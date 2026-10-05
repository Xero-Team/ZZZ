use anyhow::Result;
use buffer_diff::BufferDiff;
use editor::{
    Editor, EditorEvent, EditorSettings, MultiBuffer, SplittableEditor, multibuffer_context_lines,
};
use gpui::{
    AnyElement, App, AppContext as _, AsyncApp, Context, Entity, EventEmitter, FocusHandle,
    Focusable, Font, IntoElement, Render, SharedString, Subscription, Task, Window,
};
use i18n::tr;
use language::{Buffer, Capability, HighlightedText, OffsetRangeExt};
use multi_buffer::PathKey;
use project::Project;
use settings::Settings;
use std::{
    any::{Any, TypeId},
    path::{Path, PathBuf},
    sync::Arc,
};
use ui::{Color, Icon, IconName, Label, LabelCommon as _};
use util::paths::PathStyle;
use util::rel_path::RelPath;
use workspace::{
    Item, ItemHandle as _, ItemNavHistory, ToolbarItemLocation, Workspace,
    item::{ItemEvent, SaveOptions, TabContentParams},
    searchable::SearchableItemHandle,
};

pub struct MultiDiffView {
    editor: Entity<SplittableEditor>,
    file_count: usize,
    _editor_event_subscription: Subscription,
    _buffer_language_subscription: Subscription,
}

struct Entry {
    index: usize,
    new_path: PathBuf,
    new_buffer: Entity<Buffer>,
    diff: Entity<BufferDiff>,
}

async fn load_entries(
    diff_pairs: Vec<[String; 2]>,
    project: &Entity<Project>,
    cx: &mut AsyncApp,
) -> Result<(Vec<Entry>, Option<PathBuf>)> {
    let mut entries = Vec::with_capacity(diff_pairs.len());
    let mut all_paths = Vec::with_capacity(diff_pairs.len());

    for (ix, pair) in diff_pairs.into_iter().enumerate() {
        let old_path = PathBuf::from(&pair[0]);
        let new_path = PathBuf::from(&pair[1]);

        let old_buffer = project
            .update(cx, |project, cx| project.open_local_buffer(&old_path, cx))
            .await?;
        let new_buffer = project
            .update(cx, |project, cx| project.open_local_buffer(&new_path, cx))
            .await?;

        let diff = build_buffer_diff(&old_buffer, &new_buffer, cx).await?;

        all_paths.push(new_path.clone());
        entries.push(Entry {
            index: ix,
            new_path,
            new_buffer: new_buffer.clone(),
            diff,
        });
    }

    let common_root = common_prefix(&all_paths);
    Ok((entries, common_root))
}

fn register_entry(
    multibuffer: &Entity<MultiBuffer>,
    entry: Entry,
    common_root: &Option<PathBuf>,
    context_lines: u32,
    cx: &mut Context<Workspace>,
) {
    let snapshot = entry.new_buffer.read(cx).snapshot();
    let diff_snapshot = entry.diff.read(cx).snapshot(cx);

    let ranges: Vec<std::ops::Range<language::Point>> = diff_snapshot
        .hunks(&snapshot)
        .map(|hunk| hunk.buffer_range.to_point(&snapshot))
        .collect();

    let display_rel = common_root
        .as_ref()
        .and_then(|root| entry.new_path.strip_prefix(root).ok())
        .map_or_else(
            || {
                entry
                    .new_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .and_then(|s| RelPath::new(Path::new(s), PathStyle::Posix).ok())
                    .map_or_else(
                        || {
                            RelPath::new(Path::new("untitled"), PathStyle::Posix)
                                .expect("value should be present")
                                .into_owned()
                                .into()
                        },
                        |r| r.into_owned().into(),
                    )
            },
            |rel| {
                RelPath::new(rel, PathStyle::local()).map_or_else(
                    |_| {
                        RelPath::new(Path::new("untitled"), PathStyle::Posix)
                            .expect("value should be present")
                            .into_owned()
                            .into()
                    },
                    |r| r.into_owned().into(),
                )
            },
        );

    let path_key = PathKey::with_sort_prefix(entry.index as u64, display_rel);

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            path_key,
            entry.new_buffer.clone(),
            ranges,
            context_lines,
            cx,
        );
        multibuffer.add_diff(entry.diff.clone(), cx);
    });
}

fn common_prefix(paths: &[PathBuf]) -> Option<PathBuf> {
    let mut iter = paths.iter();
    let mut prefix = iter.next()?.clone();

    for path in iter {
        while !path.starts_with(&prefix) {
            if !prefix.pop() {
                return Some(PathBuf::new());
            }
        }
    }

    Some(prefix)
}

async fn build_buffer_diff(
    old_buffer: &Entity<Buffer>,
    new_buffer: &Entity<Buffer>,
    cx: &mut AsyncApp,
) -> Result<Entity<BufferDiff>> {
    let old_buffer_snapshot = old_buffer.read_with(cx, |buffer, _| buffer.snapshot());
    let new_buffer_snapshot = new_buffer.read_with(cx, |buffer, _| buffer.snapshot());

    let diff = cx.new(|cx| BufferDiff::new(&new_buffer_snapshot.text, cx));

    let update = diff
        .update(cx, |diff, cx| {
            diff.update_diff(
                new_buffer_snapshot.text.clone(),
                Some(old_buffer_snapshot.text().into()),
                Some(true),
                new_buffer_snapshot.language().cloned(),
                cx,
            )
        })
        .await;

    diff.update(cx, |diff, cx| {
        diff.set_snapshot(update, &new_buffer_snapshot.text, cx)
    })
    .await;

    Ok(diff)
}

impl MultiDiffView {
    pub fn open(
        diff_pairs: Vec<[String; 2]>,
        workspace: &Workspace,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<Entity<Self>>> {
        let project = workspace.project().clone();
        let workspace = workspace.weak_handle();
        let context_lines = multibuffer_context_lines(cx);

        window.spawn(cx, async move |cx| {
            let (entries, common_root) = load_entries(diff_pairs, &project, cx).await?;

            workspace.update_in(cx, |workspace, window, cx| {
                let multibuffer = cx.new(|cx| {
                    let mut multibuffer = MultiBuffer::new(Capability::ReadWrite);
                    multibuffer.set_all_diff_hunks_expanded(cx);
                    multibuffer
                });

                let file_count = entries.len();
                for entry in entries {
                    register_entry(&multibuffer, entry, &common_root, context_lines, cx);
                }

                let workspace_handle = cx.entity();
                let diff_view = cx.new(|cx| {
                    Self::new(
                        multibuffer.clone(),
                        project.clone(),
                        file_count,
                        window,
                        workspace_handle,
                        cx,
                    )
                });

                let pane = workspace.active_pane();
                pane.update(cx, |pane, cx| {
                    pane.add_item(Box::new(diff_view.clone()), true, true, None, window, cx);
                });

                // Hide the left dock (file explorer) for a cleaner diff view
                workspace.left_dock().update(cx, |dock, cx| {
                    dock.set_open(false, window, cx);
                });

                diff_view
            })
        })
    }

    fn new(
        multibuffer: Entity<MultiBuffer>,
        project: Entity<Project>,
        file_count: usize,
        window: &mut Window,
        workspace: Entity<Workspace>,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = cx.new(|cx| {
            let editor = SplittableEditor::new(
                EditorSettings::get_global(cx).diff_view_style,
                multibuffer.clone(),
                project.clone(),
                workspace,
                window,
                cx,
            );
            editor.update_editors(cx, |editor, cx| {
                editor.disable_diagnostics(cx);
                editor.set_expand_all_diff_hunks(cx);
            });
            editor.disable_diff_hunk_controls(cx);
            editor
        });

        let editor_event_subscription = cx.subscribe(&editor, |_, _, event: &EditorEvent, cx| {
            cx.emit(event.clone())
        });

        let buffer_language_subscription =
            cx.subscribe(&multibuffer, |_, multibuffer, event, cx| {
                let &multi_buffer::Event::LanguageChanged(buffer_id, _) = event else {
                    return;
                };
                let Some(rhs_buffer) = multibuffer.read(cx).buffer(buffer_id) else {
                    return;
                };
                let Some(diff) = multibuffer.read(cx).diff_for(buffer_id) else {
                    return;
                };

                let language = rhs_buffer.read(cx).language().cloned();
                let lhs_buffer = diff.read(cx).base_text_buffer().clone();
                lhs_buffer.update(cx, |lhs_buffer, cx| {
                    lhs_buffer.set_language_async(language, cx);
                });
            });

        Self {
            editor,
            file_count,
            _editor_event_subscription: editor_event_subscription,
            _buffer_language_subscription: buffer_language_subscription,
        }
    }

    fn title(&self, cx: &App) -> SharedString {
        if self.file_count == 1 {
            tr(cx, "git_ui.multi_diff_view.title_one", "Diff (1 file)").into()
        } else {
            tr(cx, "git_ui.multi_diff_view.title_many", "Diff ({} files)")
                .replacen("{}", &self.file_count.to_string(), 1)
                .into()
        }
    }
}

impl EventEmitter<EditorEvent> for MultiDiffView {}

impl Focusable for MultiDiffView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.focus_handle(cx)
    }
}

impl Item for MultiDiffView {
    type Event = EditorEvent;

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        Some(Icon::new(IconName::Diff).color(Color::Muted))
    }

    fn tab_content(&self, params: TabContentParams, _window: &Window, cx: &App) -> AnyElement {
        Label::new(self.title(cx))
            .color(if params.selected {
                Color::Default
            } else {
                Color::Muted
            })
            .into_any_element()
    }

    fn tab_tooltip_text(&self, cx: &App) -> Option<ui::SharedString> {
        Some(self.title(cx))
    }

    fn tab_content_text(&self, _detail: usize, cx: &App) -> SharedString {
        self.title(cx)
    }

    fn to_item_events(event: &EditorEvent, f: &mut dyn FnMut(ItemEvent)) {
        Editor::to_item_events(event, f)
    }

    fn deactivated(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |editor, cx| editor.deactivated(window, cx));
    }

    fn act_as_type<'a>(
        &'a self,
        type_id: TypeId,
        self_handle: &'a Entity<Self>,
        cx: &'a App,
    ) -> Option<gpui::AnyEntity> {
        if type_id == TypeId::of::<Self>() {
            Some(self_handle.clone().into())
        } else {
            self.editor.act_as_type(type_id, cx)
        }
    }

    fn as_searchable(&self, _: &Entity<Self>, _: &App) -> Option<Box<dyn SearchableItemHandle>> {
        Some(Box::new(self.editor.clone()))
    }

    fn set_nav_history(
        &mut self,
        nav_history: ItemNavHistory,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor.update(cx, |editor, cx| {
            editor.rhs_editor().update(cx, |editor, _| {
                editor.set_nav_history(Some(nav_history));
            });
        });
    }

    fn navigate(
        &mut self,
        data: Arc<dyn Any + Send>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.editor
            .update(cx, |editor, cx| editor.navigate(data, window, cx))
    }

    fn breadcrumb_location(&self, _: &App) -> ToolbarItemLocation {
        ToolbarItemLocation::PrimaryLeft
    }

    fn breadcrumbs(&self, cx: &App) -> Option<(Vec<HighlightedText>, Option<Font>)> {
        self.editor.breadcrumbs(cx)
    }

    fn added_to_workspace(
        &mut self,
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor.update(cx, |editor, cx| {
            editor.added_to_workspace(workspace, window, cx)
        });
    }

    fn can_save(&self, cx: &App) -> bool {
        self.editor.read(cx).can_save(cx)
    }

    fn save(
        &mut self,
        options: SaveOptions,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Task<Result<()>> {
        self.editor
            .update(cx, |editor, cx| editor.save(options, project, window, cx))
    }
}

impl Render for MultiDiffView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.editor.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{BorrowAppContext, TestAppContext};
    use language::{Language, LanguageConfig};
    use project::{FakeFs, Project};
    use serde_json::json;
    use settings::{DiffViewStyle, SettingsStore};
    use util::path;
    use workspace::MultiWorkspace;

    async fn test_init(
        cx: &mut TestAppContext,
        diff_style: DiffViewStyle,
    ) -> Entity<MultiDiffView> {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
            cx.update_global::<SettingsStore, _>(|store, cx| {
                store.update_user_settings(cx, |settings| {
                    settings.editor.diff_view_style = Some(diff_style);
                });
            });
            i18n::init(cx);
            theme_settings::init(theme::LoadThemes::JustBase, cx);
            editor::init(cx);
            crate::init(cx);
        });

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            path!("/test"),
            json!({
                "old": {
                    "file.rs": "fn main() {}\n",
                    "old_file.rs": "pub const BAR: usize = 0;\n",
                },
                "new": {
                    "file.rs": "fn main() { unimplemented!() }\n",
                    "new_file.rs": "pub fn foo() {}\n",
                }
            }),
        )
        .await;

        let project = Project::test(fs.clone(), [path!("/test").as_ref()], cx).await;
        let (multi_workspace, cx) =
            cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace =
            multi_workspace.read_with(cx, |multi_workspace, _| multi_workspace.workspace().clone());

        let diff_view = workspace
            .update_in(cx, |workspace, window, cx| {
                MultiDiffView::open(
                    vec![
                        [
                            path!("/test/old/file.rs").to_string(),
                            path!("/test/new/file.rs").to_string(),
                        ],
                        [
                            path!("/test/old/old_file.rs").to_string(),
                            path!("/test/new/old_file.rs").to_string(),
                        ],
                        [
                            path!("/test/old/new_file.rs").to_string(),
                            path!("/test/new/new_file.rs").to_string(),
                        ],
                    ],
                    workspace,
                    window,
                    cx,
                )
            })
            .await
            .expect("multi-diff view should open");

        cx.run_until_parked();
        diff_view
    }

    fn rust_language() -> Arc<Language> {
        Arc::new(Language::new(
            LanguageConfig {
                name: "Rust".into(),
                ..LanguageConfig::default()
            },
            None,
        ))
    }

    #[gpui::test]
    async fn test_unified_diff_view_propagates_languages(cx: &mut TestAppContext) {
        let diff_view = test_init(cx, DiffViewStyle::Unified).await;
        let language = rust_language();

        diff_view.update(cx, |diff_view, cx| {
            diff_view.editor.update(cx, |editor, cx| {
                editor.rhs_editor().update(cx, |rhs_editor, cx| {
                    rhs_editor.buffer().update(cx, |multibuffer, cx| {
                        multibuffer.for_each_buffer(&mut |buffer| {
                            buffer.update(cx, |buffer, cx| {
                                buffer.set_language(Some(language.clone()), cx);
                            });
                        });
                    });
                });
            });
        });
        cx.run_until_parked();

        diff_view.read_with(cx, |diff_view, cx| {
            let rhs_editor = diff_view.editor.read(cx).rhs_editor().clone();
            let multibuffer = rhs_editor.read(cx).buffer().read(cx);
            multibuffer.for_each_buffer(&mut |buffer| {
                let diff = multibuffer
                    .diff_for(buffer.read(cx).remote_id())
                    .expect("each buffer should have a diff");
                let language = diff
                    .read(cx)
                    .base_text_buffer()
                    .read(cx)
                    .language()
                    .map(|language| language.name());
                assert_eq!(language, Some("Rust".into()));
            });
        });
    }

    #[gpui::test]
    async fn test_split_diff_view(cx: &mut TestAppContext) {
        let diff_view = test_init(cx, DiffViewStyle::Split).await;
        let language = rust_language();

        diff_view.update(cx, |diff_view, cx| {
            diff_view.editor.update(cx, |editor, cx| {
                editor.rhs_editor().update(cx, |rhs_editor, cx| {
                    rhs_editor.buffer().update(cx, |multibuffer, cx| {
                        multibuffer.for_each_buffer(&mut |buffer| {
                            buffer.update(cx, |buffer, cx| {
                                buffer.set_language(Some(language.clone()), cx);
                            });
                        });
                    });
                });
            });
        });
        cx.run_until_parked();

        diff_view.read_with(cx, |diff_view, cx| {
            let lhs_editor = diff_view
                .editor
                .read(cx)
                .lhs_editor()
                .expect("diff view should be split")
                .clone();
            lhs_editor
                .read(cx)
                .buffer()
                .read(cx)
                .for_each_buffer(&mut |buffer| {
                    let lhs_language = buffer.read(cx).language().map(|language| language.name());
                    assert_eq!(lhs_language, Some("Rust".into()));
                });
        });
    }
}
