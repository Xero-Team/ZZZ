use std::any::TypeId;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};
use editor::{Editor, EditorEvent, EditorSettingsScrollbarProxy};
use file_icons::FileIcons;
use futures::future;
use gpui::{
    App, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle, Focusable, IsZero, Render,
    RenderImage, ScrollHandle, SharedString, Subscription, Task, WeakEntity, Window, img, point,
};
use i18n::tr;
use project::{Project, ProjectPath, WorktreeId};
use theme::GlobalTheme;
use typst::syntax::package::PackageSpec;
use ui::{Banner, ScrollAxes, Scrollbars, Severity, WithScrollbar, prelude::*};
use util::ResultExt as _;
use util::paths::PathStyle;
use util::rel_path::RelPath;
use workspace::item::{Item, ItemBufferKind, SaveOptions, SerializableItem};
use workspace::notifications::{NotificationId, NotifyResultExt};
use workspace::{ItemId, Pane, SaveIntent, Toast, Workspace, WorkspaceId, delete_unloaded_items};

use crate::typst_world::{
    CompileAttempt, CompileResult, PreviewColors, TypstCompiler,
    download_package as download_typst_package,
};
use crate::{
    CloseAndReturnToEditor, OpenFollowingPreview, OpenPreview, OpenPreviewToTheSide, ScrollDown,
    ScrollPageDown, ScrollPageUp, ScrollToBottom, ScrollToTop, ScrollUp,
};

const MAX_REMOTE_LOAD_PASSES: usize = 32;

pub struct TypstPreviewView {
    workspace: WeakEntity<Workspace>,
    project: Entity<Project>,
    active_editor: Option<EditorState>,
    source: Option<SourceDescriptor>,
    preview_colors: PreviewColors,
    compiler_key: Option<CompilerKey>,
    compiler: Option<TypstCompiler>,
    pages: Vec<Arc<RenderImage>>,
    error: Option<SharedString>,
    warning: Option<SharedString>,
    is_compiling: bool,
    is_stale: bool,
    requested_generation: u64,
    compile_task: Option<Task<()>>,
    package_download_task: Option<Task<()>>,
    offered_package: Option<PackageSpec>,
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
    mode: TypstPreviewMode,
}

struct EditorState {
    editor: Entity<Editor>,
    _subscription: Subscription,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SourceDescriptor {
    worktree_id: WorktreeId,
    relative_path: Arc<RelPath>,
    root: PathBuf,
    remote: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CompilerKey {
    source: SourceDescriptor,
    colors: PreviewColors,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypstPreviewMode {
    Default,
    Follow,
}

impl TypstPreviewMode {
    fn to_db(self) -> i64 {
        match self {
            Self::Default => 0,
            Self::Follow => 1,
        }
    }

    fn from_db(value: i64) -> Self {
        match value {
            1 => Self::Follow,
            _ => Self::Default,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TypstPreviewEvent {
    SourceEditorChanged,
    SourceFileHandleChanged,
}

impl TypstPreviewView {
    pub fn register(workspace: &mut Workspace, _window: &mut Window, _cx: &mut Context<Workspace>) {
        workspace.register_action(|workspace, _: &OpenPreview, window, cx| {
            let Some(editor) = Self::resolve_active_item_as_typst_editor(workspace, cx) else {
                return;
            };
            if !Self::ensure_saved_file(&editor, workspace, cx) {
                return;
            }

            let view = Self::create_view(
                TypstPreviewMode::Default,
                workspace,
                editor.clone(),
                window,
                cx,
            );
            workspace.active_pane().update(cx, |pane, cx| {
                if let Some(index) = Self::find_existing_preview_item(pane, &editor, cx) {
                    pane.activate_item(index, true, true, window, cx);
                } else {
                    pane.add_item(Box::new(view), true, true, None, window, cx);
                }
            });
            cx.notify();
        });

        workspace.register_action(|workspace, _: &OpenPreviewToTheSide, window, cx| {
            let Some(editor) = Self::resolve_active_item_as_typst_editor(workspace, cx) else {
                return;
            };
            if !Self::ensure_saved_file(&editor, workspace, cx) {
                return;
            }

            let view = Self::create_view(
                TypstPreviewMode::Default,
                workspace,
                editor.clone(),
                window,
                cx,
            );
            let pane = workspace
                .find_pane_in_direction(workspace::SplitDirection::Right, cx)
                .unwrap_or_else(|| {
                    workspace.split_pane(
                        workspace.active_pane().clone(),
                        workspace::SplitDirection::Right,
                        window,
                        cx,
                    )
                });
            pane.update(cx, |pane, cx| {
                if let Some(index) = Self::find_existing_preview_item(pane, &editor, cx) {
                    pane.activate_item(index, true, true, window, cx);
                } else {
                    pane.add_item(Box::new(view), false, false, None, window, cx);
                }
            });
            editor.focus_handle(cx).focus(window, cx);
            cx.notify();
        });

        workspace.register_action(|workspace, _: &OpenFollowingPreview, window, cx| {
            let Some(editor) = Self::resolve_active_item_as_typst_editor(workspace, cx) else {
                return;
            };
            if !Self::ensure_saved_file(&editor, workspace, cx) {
                return;
            }

            let existing = {
                let pane = workspace.active_pane().read(cx);
                pane.items_of_type::<Self>()
                    .find(|view| view.read(cx).mode == TypstPreviewMode::Follow)
                    .and_then(|view| pane.index_for_item(&view))
            };
            if let Some(index) = existing {
                workspace.active_pane().update(cx, |pane, cx| {
                    pane.activate_item(index, true, true, window, cx);
                });
            } else {
                let view =
                    Self::create_view(TypstPreviewMode::Follow, workspace, editor, window, cx);
                workspace.active_pane().update(cx, |pane, cx| {
                    pane.add_item(Box::new(view), true, true, None, window, cx);
                });
            }
            cx.notify();
        });
    }

    pub fn resolve_active_item_as_typst_editor(
        workspace: &Workspace,
        cx: &mut Context<Workspace>,
    ) -> Option<Entity<Editor>> {
        workspace
            .active_item(cx)
            .and_then(|item| item.act_as::<Editor>(cx))
            .filter(|editor| Self::is_typst_file(editor, cx))
    }

    pub fn is_typst_path(path: impl AsRef<Path>) -> bool {
        path.as_ref()
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("typ"))
    }

    pub fn open_for_project_path(
        project_path: ProjectPath,
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let open_buffer = workspace
            .project()
            .update(cx, |project, cx| project.open_buffer(project_path, cx));

        cx.spawn_in(window, async move |workspace, mut cx| {
            let Some(buffer) = open_buffer
                .await
                .notify_workspace_async_err(workspace.clone(), &mut cx)
            else {
                return;
            };
            workspace
                .update_in(cx, |workspace, window, cx| {
                    let project = workspace.project().clone();
                    let editor = cx.new(|cx| Editor::for_buffer(buffer, Some(project), window, cx));
                    let preview =
                        Self::create_view(TypstPreviewMode::Default, workspace, editor, window, cx);
                    workspace.active_pane().update(cx, |pane, cx| {
                        pane.add_item(Box::new(preview), true, true, None, window, cx);
                    });
                })
                .log_err();
        })
        .detach();
    }

    fn ensure_saved_file(
        editor: &Entity<Editor>,
        workspace: &mut Workspace,
        cx: &mut Context<Workspace>,
    ) -> bool {
        let is_saved = editor
            .read(cx)
            .buffer()
            .read(cx)
            .as_singleton()
            .and_then(|buffer| buffer.read(cx).file().cloned())
            .is_some_and(|file| file.disk_state().exists());
        if !is_saved {
            workspace.show_toast(
                Toast::new(
                    NotificationId::unique::<UnsavedTypstPreview>(),
                    tr(
                        cx,
                        "typst_preview.save_before_opening",
                        "Save the Typst file before opening its preview.",
                    ),
                )
                .autohide(),
                cx,
            );
        }
        is_saved
    }

    fn find_existing_preview_item(pane: &Pane, editor: &Entity<Editor>, cx: &App) -> Option<usize> {
        let target_buffer = editor.read(cx).buffer().read(cx).as_singleton()?;
        pane.items_of_type::<Self>()
            .find(|view| {
                let view = view.read(cx);
                view.mode == TypstPreviewMode::Default
                    && view.active_editor.as_ref().is_some_and(|active| {
                        active
                            .editor
                            .read(cx)
                            .buffer()
                            .read(cx)
                            .as_singleton()
                            .as_ref()
                            == Some(&target_buffer)
                    })
            })
            .and_then(|view| pane.index_for_item(&view))
    }

    fn create_view(
        mode: TypstPreviewMode,
        workspace: &mut Workspace,
        editor: Entity<Editor>,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        let project = workspace.project().clone();
        let workspace = workspace.weak_handle();
        Self::new(mode, editor, project, workspace, window, cx)
    }

    fn new(
        mode: TypstPreviewMode,
        editor: Entity<Editor>,
        project: Entity<Project>,
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let preview_colors = Self::preview_colors(cx);
            let mut view = Self {
                workspace: workspace.clone(),
                project,
                active_editor: None,
                source: None,
                preview_colors,
                compiler_key: None,
                compiler: None,
                pages: Vec::new(),
                error: None,
                warning: None,
                is_compiling: false,
                is_stale: false,
                requested_generation: 0,
                compile_task: None,
                package_download_task: None,
                offered_package: None,
                focus_handle: cx.focus_handle(),
                scroll_handle: ScrollHandle::new(),
                mode,
            };
            view.set_editor(editor, window, cx);

            cx.observe_global_in::<GlobalTheme>(window, |view, window, cx| {
                let preview_colors = Self::preview_colors(cx);
                if view.preview_colors != preview_colors {
                    view.preview_colors = preview_colors;
                    view.request_compile(window, cx);
                }
                cx.notify();
            })
            .detach();

            if let Some(workspace) = workspace.upgrade() {
                match mode {
                    TypstPreviewMode::Follow => {
                        cx.subscribe_in(
                            &workspace,
                            window,
                            |view, workspace, event, window, cx| {
                                if matches!(event, workspace::Event::ActiveItemChanged)
                                    && let Some(editor) = workspace
                                        .read(cx)
                                        .active_item(cx)
                                        .and_then(|item| item.act_as::<Editor>(cx))
                                    && Self::is_typst_file(&editor, cx)
                                {
                                    view.set_editor(editor, window, cx);
                                }
                            },
                        )
                        .detach();
                    }
                    TypstPreviewMode::Default => {
                        cx.subscribe_in(
                            &workspace,
                            window,
                            |view, workspace, event, window, cx| {
                                if matches!(
                                    event,
                                    workspace::Event::ItemAdded { .. }
                                        | workspace::Event::ItemRemoved { .. }
                                ) && let Some(editor) =
                                    view.find_canonical_editor(workspace.read(cx), cx)
                                {
                                    view.set_editor(editor, window, cx);
                                }
                            },
                        )
                        .detach();
                    }
                }
            }

            view
        })
    }

    fn is_typst_file<V>(editor: &Entity<Editor>, cx: &mut Context<V>) -> bool {
        editor
            .read(cx)
            .buffer()
            .read(cx)
            .as_singleton()
            .and_then(|buffer| buffer.read(cx).language())
            .is_some_and(|language| language.name().as_ref() == "Typst")
    }

    fn set_editor(&mut self, editor: Entity<Editor>, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .active_editor
            .as_ref()
            .is_some_and(|active| active.editor == editor)
        {
            return;
        }

        let had_editor = self.active_editor.is_some();
        let subscription = cx.subscribe_in(
            &editor,
            window,
            |view, editor, event: &EditorEvent, window, cx| match event {
                EditorEvent::Saved | EditorEvent::FileHandleChanged => {
                    view.refresh_source(editor, window, cx);
                    view.request_compile(window, cx);
                    if matches!(event, EditorEvent::FileHandleChanged) {
                        cx.emit(TypstPreviewEvent::SourceFileHandleChanged);
                    }
                }
                EditorEvent::Edited { .. }
                | EditorEvent::BufferEdited { .. }
                | EditorEvent::BuffersEdited { .. }
                | EditorEvent::DirtyChanged => {
                    view.is_stale = editor
                        .read(cx)
                        .buffer()
                        .read(cx)
                        .as_singleton()
                        .is_some_and(|buffer| buffer.read(cx).is_dirty());
                    cx.notify();
                }
                _ => {}
            },
        );

        self.active_editor = Some(EditorState {
            editor: editor.clone(),
            _subscription: subscription,
        });
        self.refresh_source(&editor, window, cx);
        self.is_stale = editor
            .read(cx)
            .buffer()
            .read(cx)
            .as_singleton()
            .is_some_and(|buffer| buffer.read(cx).is_dirty());
        self.request_compile(window, cx);
        if had_editor {
            cx.emit(TypstPreviewEvent::SourceEditorChanged);
        }
    }

    fn refresh_source(
        &mut self,
        editor: &Entity<Editor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let source = Self::source_descriptor(editor, &self.project, cx);
        if self.source != source {
            for page in self.pages.drain(..) {
                cx.drop_image(page, Some(window));
            }
            self.source = source;
            self.compiler = None;
            self.compiler_key = None;
            self.error = None;
            self.warning = None;
            self.offered_package = None;
            self.requested_generation = self.requested_generation.wrapping_add(1);
        }
    }

    fn source_descriptor(
        editor: &Entity<Editor>,
        project: &Entity<Project>,
        cx: &App,
    ) -> Option<SourceDescriptor> {
        let buffer = editor.read(cx).buffer().read(cx).as_singleton()?;
        let buffer = buffer.read(cx);
        let file = buffer.file()?;
        if !file.disk_state().exists() {
            return None;
        }
        let worktree_id = file.worktree_id(cx);
        let worktree = project.read(cx).worktree_for_id(worktree_id, cx)?;
        let worktree = worktree.read(cx);
        Some(SourceDescriptor {
            worktree_id,
            relative_path: file.path().clone(),
            root: worktree.abs_path().to_path_buf(),
            remote: worktree.is_remote(),
        })
    }

    fn request_compile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.requested_generation = self.requested_generation.wrapping_add(1);
        self.is_stale = self
            .active_editor
            .as_ref()
            .and_then(|active| active.editor.read(cx).buffer().read(cx).as_singleton())
            .is_some_and(|buffer| buffer.read(cx).is_dirty());
        if self.compile_task.is_some() {
            return;
        }
        self.start_compile(window, cx);
    }

    fn preview_colors(cx: &App) -> PreviewColors {
        let background = u32::from(cx.theme().colors().editor_background.to_rgb()) >> 8;
        let foreground = u32::from(cx.theme().colors().editor_foreground.to_rgb()) >> 8;
        PreviewColors::new(background, foreground)
    }

    fn start_compile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(source) = self.source.clone() else {
            self.error = Some(
                tr(
                    cx,
                    "typst_preview.save_before_opening",
                    "Save the Typst file before opening its preview.",
                )
                .into(),
            );
            self.is_compiling = false;
            cx.notify();
            return;
        };

        let compiler_key = CompilerKey {
            source: source.clone(),
            colors: self.preview_colors,
        };
        let compiler = self
            .compiler
            .take()
            .filter(|_| self.compiler_key.as_ref() == Some(&compiler_key));
        let compiler = match compiler.map_or_else(
            || {
                TypstCompiler::new(
                    source.root.clone(),
                    source.relative_path.as_unix_str(),
                    source.remote,
                    self.preview_colors,
                )
            },
            Ok,
        ) {
            Ok(compiler) => compiler,
            Err(error) => {
                self.error = Some(error.to_string().into());
                self.is_compiling = false;
                cx.notify();
                return;
            }
        };

        self.compiler_key = Some(compiler_key.clone());
        self.is_compiling = true;
        let generation = self.requested_generation;
        let project = self.project.clone();
        let renderer = cx.svg_renderer();
        self.compile_task = Some(cx.spawn_in(window, async move |view, mut cx| {
            let (compiler, result) =
                compile_saved_document(compiler, source.clone(), project, renderer, &mut cx).await;

            view.update_in(cx, |view, window, cx| {
                let compiler_is_current = view.source.as_ref() == Some(&source)
                    && view.preview_colors == compiler_key.colors;
                if compiler_is_current {
                    view.compiler = Some(compiler);
                    view.compiler_key = Some(compiler_key);
                }
                view.compile_task = None;

                if compiler_is_current && view.requested_generation == generation {
                    view.apply_compile_result(result, window, cx);
                    view.is_compiling = false;
                } else {
                    view.start_compile(window, cx);
                }
                cx.notify();
            })
            .log_err();
        }));
    }

    fn apply_compile_result(
        &mut self,
        result: Result<CompileResult>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(result) => {
                self.warning = result.warnings.map(Into::into);
                if let Some(package) = result.missing_packages.first().cloned() {
                    self.offer_package_download(package, cx);
                } else {
                    self.offered_package = None;
                }

                match result.pages {
                    Ok(pages) => {
                        for page in self.pages.drain(..) {
                            cx.drop_image(page, Some(window));
                        }
                        self.pages = pages;
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error.into()),
                }
            }
            Err(error) => {
                self.warning = None;
                self.offered_package = None;
                self.error = Some(error.to_string().into());
            }
        }
    }

    fn offer_package_download(&mut self, package: PackageSpec, cx: &mut Context<Self>) {
        if self.offered_package.as_ref() == Some(&package) {
            return;
        }
        self.offered_package = Some(package.clone());

        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let view = cx.weak_entity();
        let package_name = package.to_string();
        let message = tr(
            cx,
            "typst_preview.package_missing",
            "Typst package {} is not installed.",
        )
        .replacen("{}", &package_name, 1);
        let download = tr(cx, "typst_preview.download_package", "Download");
        workspace.update(cx, |workspace, cx| {
            workspace.show_toast(
                Toast::new(
                    NotificationId::Named(format!("typst-package-{package_name}").into()),
                    message,
                )
                .on_click(download, move |window, cx| {
                    view.update(cx, |view, cx| {
                        view.download_package(package.clone(), window, cx);
                    })
                    .log_err();
                }),
                cx,
            );
        });
    }

    fn download_package(
        &mut self,
        package: PackageSpec,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.package_download_task.is_some() {
            return;
        }

        let package_name = package.to_string();
        self.package_download_task = Some(cx.spawn_in(window, async move |view, cx| {
            let download = cx.background_spawn(async move { download_typst_package(&package) });
            let result = download.await;
            view.update_in(cx, |view, window, cx| {
                view.package_download_task = None;
                let Some(workspace) = view.workspace.upgrade() else {
                    return;
                };
                match result {
                    Ok(()) => {
                        view.offered_package = None;
                        workspace.update(cx, |workspace, cx| {
                            workspace.show_toast(
                                Toast::new(
                                    NotificationId::unique::<TypstPackageDownloaded>(),
                                    tr(
                                        cx,
                                        "typst_preview.package_downloaded",
                                        "Downloaded Typst package {}.",
                                    )
                                    .replacen(
                                        "{}",
                                        &package_name,
                                        1,
                                    ),
                                )
                                .autohide(),
                                cx,
                            );
                        });
                        view.request_compile(window, cx);
                    }
                    Err(error) => {
                        workspace.update(cx, |workspace, cx| {
                            workspace.show_toast(
                                Toast::new(
                                    NotificationId::unique::<TypstPackageDownloadFailed>(),
                                    tr(
                                        cx,
                                        "typst_preview.package_download_failed",
                                        "Failed to download Typst package {}: {}",
                                    )
                                    .replacen("{}", &package_name, 1)
                                    .replacen(
                                        "{}",
                                        &error.to_string(),
                                        1,
                                    ),
                                ),
                                cx,
                            );
                        });
                    }
                }
            })
            .log_err();
        }));
    }

    fn find_canonical_editor(&self, workspace: &Workspace, cx: &App) -> Option<Entity<Editor>> {
        let current = self.active_editor.as_ref()?.editor.clone();
        let target_buffer = current.read(cx).buffer().read(cx).as_singleton()?;
        workspace.items_of_type::<Editor>(cx).find(|editor| {
            editor.read(cx).buffer().read(cx).as_singleton().as_ref() == Some(&target_buffer)
        })
    }

    fn close_and_return_to_editor(
        &mut self,
        _: &CloseAndReturnToEditor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self
            .active_editor
            .as_ref()
            .map(|active| active.editor.clone())
        else {
            return;
        };
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let preview_id = cx.entity_id();

        window.defer(cx, move |window, cx| {
            workspace.update(cx, |workspace, cx| {
                if !workspace.activate_item(&editor, true, true, window, cx) {
                    workspace.add_item_to_active_pane(Box::new(editor), None, true, window, cx);
                }
                if let Some(pane) = workspace.pane_for_item_id(preview_id) {
                    pane.update(cx, |pane, cx| {
                        pane.close_item_by_id(preview_id, SaveIntent::Skip, window, cx)
                    })
                    .detach_and_log_err(cx);
                }
            });
        });
    }

    fn scroll_by(&self, distance: Pixels) {
        let offset = self.scroll_handle.offset();
        self.scroll_handle
            .set_offset(point(offset.x, offset.y - distance));
    }

    fn scroll_page_up(&mut self, _: &ScrollPageUp, _window: &mut Window, cx: &mut Context<Self>) {
        let height = self.scroll_handle.bounds().size.height;
        if !height.is_zero() {
            self.scroll_by(-height);
            cx.notify();
        }
    }

    fn scroll_page_down(
        &mut self,
        _: &ScrollPageDown,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let height = self.scroll_handle.bounds().size.height;
        if !height.is_zero() {
            self.scroll_by(height);
            cx.notify();
        }
    }

    fn scroll_up(&mut self, _: &ScrollUp, window: &mut Window, cx: &mut Context<Self>) {
        self.scroll_by(-window.rem_size());
        cx.notify();
    }

    fn scroll_down(&mut self, _: &ScrollDown, window: &mut Window, cx: &mut Context<Self>) {
        self.scroll_by(window.rem_size());
        cx.notify();
    }

    fn scroll_to_top(&mut self, _: &ScrollToTop, _window: &mut Window, cx: &mut Context<Self>) {
        self.scroll_handle.scroll_to_item(0);
        cx.notify();
    }

    fn scroll_to_bottom(
        &mut self,
        _: &ScrollToBottom,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.scroll_handle.scroll_to_bottom();
        cx.notify();
    }
}

async fn compile_saved_document(
    mut compiler: TypstCompiler,
    source: SourceDescriptor,
    project: Entity<Project>,
    renderer: gpui::SvgRenderer,
    cx: &mut AsyncWindowContext,
) -> (TypstCompiler, Result<CompileResult>) {
    if source.remote {
        compiler.clear_remote_files();
    }

    for _ in 0..MAX_REMOTE_LOAD_PASSES {
        let renderer = renderer.clone();
        let compile_task = cx.background_spawn(async move {
            let result = compiler.compile(&renderer);
            (compiler, result)
        });
        let (returned_compiler, result) = compile_task.await;
        compiler = returned_compiler;

        match result {
            CompileAttempt::Complete(result) => return (compiler, Ok(result)),
            CompileAttempt::NeedsRemoteFiles(paths) => {
                let mut reads = Vec::with_capacity(paths.len());
                for path in paths {
                    let relative_path =
                        match RelPath::new(Path::new(path.get_without_slash()), PathStyle::Posix) {
                            Ok(path) => path.into_arc(),
                            Err(error) => return (compiler, Err(error)),
                        };
                    let read = project.update(cx, |project, cx| {
                        project.read_file_bytes(source.worktree_id, relative_path.clone(), cx)
                    });
                    reads.push(async move {
                        let bytes = read.await.with_context(|| {
                            format!(
                                "failed to read remote Typst file {}",
                                relative_path.as_unix_str()
                            )
                        })?;
                        anyhow::Ok((path, bytes))
                    });
                }
                for result in future::join_all(reads).await {
                    match result {
                        Ok((path, bytes)) => compiler.insert_remote_file(path, bytes),
                        Err(error) => return (compiler, Err(error)),
                    }
                }
            }
        }
    }

    (
        compiler,
        Err(anyhow!(
            "Typst exceeded the remote import resolution limit of {MAX_REMOTE_LOAD_PASSES} passes"
        )),
    )
}

impl Focusable for TypstPreviewView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<TypstPreviewEvent> for TypstPreviewView {}

impl Item for TypstPreviewView {
    type Event = TypstPreviewEvent;

    fn act_as_type<'a>(
        &'a self,
        type_id: TypeId,
        self_handle: &'a Entity<Self>,
        _cx: &'a App,
    ) -> Option<gpui::AnyEntity> {
        if type_id == TypeId::of::<Self>() {
            Some(self_handle.clone().into())
        } else if type_id == TypeId::of::<Editor>() {
            self.active_editor
                .as_ref()
                .map(|active| active.editor.clone().into())
        } else {
            None
        }
    }

    fn tab_icon(&self, _window: &Window, cx: &App) -> Option<Icon> {
        self.active_editor
            .as_ref()
            .and_then(|active| active.editor.read(cx).buffer().read(cx).as_singleton())
            .and_then(|buffer| buffer.read(cx).file().cloned())
            .and_then(|file| FileIcons::get_icon(file.path().as_std_path(), cx))
            .map(Icon::from_path)
            .or_else(|| Some(Icon::new(IconName::FileDoc)))
    }

    fn tab_content_text(&self, _detail: usize, cx: &App) -> SharedString {
        self.active_editor.as_ref().map_or_else(
            || tr(cx, "typst_preview.tab_title", "Typst Preview").into(),
            |active| {
                let title = active.editor.read(cx).buffer().read(cx).title(cx);
                tr(cx, "typst_preview.preview_title", "Preview {}")
                    .replacen("{}", &title, 1)
                    .into()
            },
        )
    }

    fn tab_tooltip_text(&self, cx: &App) -> Option<SharedString> {
        self.active_editor
            .as_ref()?
            .editor
            .read(cx)
            .tab_tooltip_text(cx)
    }

    fn added_to_workspace(
        &mut self,
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.mode == TypstPreviewMode::Default
            && let Some(editor) = self.find_canonical_editor(workspace, cx)
        {
            self.set_editor(editor, window, cx);
        }
    }

    fn can_save(&self, cx: &App) -> bool {
        self.active_editor
            .as_ref()
            .is_some_and(|active| active.editor.read(cx).can_save(cx))
    }

    fn can_save_as(&self, cx: &App) -> bool {
        self.active_editor
            .as_ref()
            .is_some_and(|active| active.editor.read(cx).can_save_as(cx))
    }

    fn save(
        &mut self,
        options: SaveOptions,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        self.active_editor.as_ref().map_or_else(
            || Task::ready(Ok(())),
            |active| {
                active
                    .editor
                    .update(cx, |editor, cx| editor.save(options, project, window, cx))
            },
        )
    }

    fn save_as(
        &mut self,
        project: Entity<Project>,
        path: ProjectPath,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        self.active_editor.as_ref().map_or_else(
            || Task::ready(Ok(())),
            |active| {
                active
                    .editor
                    .update(cx, |editor, cx| editor.save_as(project, path, window, cx))
            },
        )
    }

    fn reload(
        &mut self,
        _project: Entity<Project>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        Task::ready(Ok(()))
    }

    fn to_item_events(event: &Self::Event, emit: &mut dyn FnMut(workspace::item::ItemEvent)) {
        match event {
            TypstPreviewEvent::SourceEditorChanged | TypstPreviewEvent::SourceFileHandleChanged => {
                emit(workspace::item::ItemEvent::UpdateTab);
                emit(workspace::item::ItemEvent::UpdateBreadcrumbs);
            }
        }
    }

    fn buffer_kind(&self, _cx: &App) -> ItemBufferKind {
        ItemBufferKind::Singleton
    }
}

impl Render for TypstPreviewView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = if self.is_stale {
            Some((
                Severity::Warning,
                tr(
                    cx,
                    "typst_preview.save_to_refresh",
                    "Save the file to refresh the Typst preview.",
                ),
            ))
        } else if self.is_compiling && self.pages.is_empty() {
            Some((
                Severity::Info,
                tr(cx, "typst_preview.compiling", "Compiling Typst preview…"),
            ))
        } else if let Some(error) = &self.error {
            Some((
                Severity::Error,
                tr(
                    cx,
                    "typst_preview.compile_failed",
                    "Typst compilation failed: {}",
                )
                .replacen("{}", error, 1),
            ))
        } else {
            self.warning.as_ref().map(|warning| {
                (
                    Severity::Warning,
                    tr(
                        cx,
                        "typst_preview.compiled_with_warnings",
                        "Typst compiled with warnings: {}",
                    )
                    .replacen("{}", warning, 1),
                )
            })
        };

        div()
            .id("TypstPreview")
            .key_context("TypstPreview")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::close_and_return_to_editor))
            .on_action(cx.listener(Self::scroll_page_up))
            .on_action(cx.listener(Self::scroll_page_down))
            .on_action(cx.listener(Self::scroll_up))
            .on_action(cx.listener(Self::scroll_down))
            .on_action(cx.listener(Self::scroll_to_top))
            .on_action(cx.listener(Self::scroll_to_bottom))
            .size_full()
            .min_h_0()
            .bg(cx.theme().colors().editor_background)
            .text_color(cx.theme().colors().text)
            .child(
                v_flex()
                    .size_full()
                    .when_some(status, |content, (severity, status)| {
                        content.child(
                            div().p_2().child(
                                Banner::new()
                                    .severity(severity)
                                    .wrap_content(true)
                                    .child(Label::new(status)),
                            ),
                        )
                    })
                    .child(
                        v_flex()
                            .id("typst-preview-scroll-container")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll_handle)
                            .p_4()
                            .gap_4()
                            .items_center()
                            .children(self.pages.iter().cloned().map(|page| {
                                div().w_full().flex().justify_center().child(
                                    img(page).max_w_full().with_fallback(|| {
                                        Icon::new(IconName::Warning).into_any_element()
                                    }),
                                )
                            })),
                    ),
            )
            .custom_scrollbars(
                Scrollbars::for_settings::<EditorSettingsScrollbarProxy>()
                    .show_along(ScrollAxes::Vertical)
                    .tracked_scroll_handle(&self.scroll_handle),
                window,
                cx,
            )
    }
}

impl SerializableItem for TypstPreviewView {
    fn serialized_item_kind() -> &'static str {
        "TypstPreviewView"
    }

    fn deserialize(
        project: Entity<Project>,
        workspace: WeakEntity<Workspace>,
        workspace_id: WorkspaceId,
        item_id: ItemId,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<Entity<Self>>> {
        let database = persistence::TypstPreviewDb::global(cx);
        window.spawn(cx, async move |cx| {
            let (absolute_path, mode) = database
                .get_preview(item_id, workspace_id)?
                .context("No Typst preview entry found")?;
            let (worktree, relative_path) = project
                .update(cx, |project, cx| {
                    project.find_or_create_worktree(absolute_path, false, cx)
                })
                .await
                .context("Typst preview path not found")?;
            let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());
            let buffer = project
                .update(cx, |project, cx| {
                    project.open_buffer(
                        ProjectPath {
                            worktree_id,
                            path: relative_path,
                        },
                        cx,
                    )
                })
                .await?;

            cx.update(|window, cx| {
                let editor =
                    cx.new(|cx| Editor::for_buffer(buffer, Some(project.clone()), window, cx));
                Self::new(
                    TypstPreviewMode::from_db(mode),
                    editor,
                    project,
                    workspace,
                    window,
                    cx,
                )
            })
        })
    }

    fn cleanup(
        workspace_id: WorkspaceId,
        alive_items: Vec<ItemId>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<()>> {
        let database = persistence::TypstPreviewDb::global(cx);
        delete_unloaded_items(alive_items, workspace_id, "typst_previews", &database, cx)
    }

    fn serialize(
        &mut self,
        workspace: &mut Workspace,
        item_id: ItemId,
        _closing: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<()>>> {
        let workspace_id = workspace.database_id()?;
        let source = self.source.as_ref()?;
        let absolute_path = workspace
            .project()
            .read(cx)
            .worktree_for_id(source.worktree_id, cx)?
            .read(cx)
            .absolutize(&source.relative_path);
        let mode = self.mode.to_db();
        let database = persistence::TypstPreviewDb::global(cx);
        Some(cx.background_spawn(async move {
            database
                .save_preview(item_id, workspace_id, absolute_path, mode)
                .await
        }))
    }

    fn should_serialize(&self, event: &Self::Event) -> bool {
        matches!(
            event,
            TypstPreviewEvent::SourceEditorChanged | TypstPreviewEvent::SourceFileHandleChanged
        )
    }
}

struct UnsavedTypstPreview;
struct TypstPackageDownloaded;
struct TypstPackageDownloadFailed;

mod persistence {
    use std::path::PathBuf;

    use db::{
        query,
        sqlez::{domain::Domain, thread_safe_connection::ThreadSafeConnection},
        sqlez_macros::sql,
    };
    use workspace::{ItemId, WorkspaceDb, WorkspaceId};

    pub struct TypstPreviewDb(ThreadSafeConnection);

    impl Domain for TypstPreviewDb {
        const NAME: &str = stringify!(TypstPreviewDb);
        const MIGRATIONS: &[&str] = &[sql!(
            CREATE TABLE typst_previews (
                workspace_id INTEGER,
                item_id INTEGER,
                abs_path BLOB,
                mode INTEGER NOT NULL DEFAULT 0,

                PRIMARY KEY(workspace_id, item_id),
                FOREIGN KEY(workspace_id) REFERENCES workspaces(workspace_id)
                ON DELETE CASCADE
            ) STRICT;
        )];
    }

    db::static_connection!(TypstPreviewDb, [WorkspaceDb]);

    impl TypstPreviewDb {
        query! {
            pub async fn save_preview(
                item_id: ItemId,
                workspace_id: WorkspaceId,
                absolute_path: PathBuf,
                mode: i64
            ) -> Result<()> {
                INSERT OR REPLACE INTO typst_previews(item_id, workspace_id, abs_path, mode)
                VALUES (?, ?, ?, ?)
            }
        }

        query! {
            pub fn get_preview(item_id: ItemId, workspace_id: WorkspaceId) -> Result<Option<(PathBuf, i64)>> {
                SELECT abs_path, mode
                FROM typst_previews
                WHERE item_id = ? AND workspace_id = ?
            }
        }
    }
}
