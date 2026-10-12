use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ::settings::Settings as _;
use futures::StreamExt as _;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{
    Action, App, AppContext as _, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle,
    Focusable, InteractiveElement, IntoElement, ParentElement, PathPromptOptions, Pixels, Render,
    SharedString, StatefulInteractiveElement, Styled, WeakEntity, Window, div,
};
use i18n::tr;
use project::ProjectPath;
use project_panel::ProjectPanel;
use ui::{Button, ContextMenu, IconName, Label, PopoverMenu, Tooltip, prelude::*};
use util::ResultExt as _;
use util::rel_path::RelPath;
use workspace::{
    Pane, Toast, Workspace,
    dock::{DockPosition, Panel, PanelEvent},
    notifications::NotificationId,
};
use worktree::Worktree;

use crate::ToggleFilePanel;
use crate::local_transfer::{download_from_session, upload_to_session};
use crate::monitor::render_monitor;
use crate::session::{Event as SessionEvent, SessionId, SessionRoot, SessionStore};
use crate::transfer::{
    copy_between_sessions, mount_id_for_session, mount_id_for_session_root, move_between_sessions,
};

pub(crate) const SESSION_FILE_PANEL_KEY: &str = "SessionFilePanel";

/// A file transfer surfaced through progress toasts.
#[derive(Clone, Copy)]
enum TransferOperation {
    Copy,
    Move,
    Download,
    Upload,
}

impl TransferOperation {
    fn in_progress(self, cx: &App) -> String {
        match self {
            Self::Copy => tr(cx, "terminal_mode.copy.in_progress", "Copying…"),
            Self::Move => tr(cx, "terminal_mode.move.in_progress", "Moving…"),
            Self::Download => tr(cx, "terminal_mode.download.in_progress", "Downloading…"),
            Self::Upload => tr(cx, "terminal_mode.upload.in_progress", "Uploading…"),
        }
    }

    fn progress(self, cx: &App, bytes_copied: u64, total_bytes: u64) -> String {
        let template = match self {
            Self::Copy => tr(cx, "terminal_mode.copy.progress", "Copying {} of {} bytes…"),
            Self::Move => tr(cx, "terminal_mode.move.progress", "Moving {} of {} bytes…"),
            Self::Download => tr(
                cx,
                "terminal_mode.download.progress",
                "Downloading {} of {} bytes…",
            ),
            Self::Upload => tr(
                cx,
                "terminal_mode.upload.progress",
                "Uploading {} of {} bytes…",
            ),
        };
        template
            .replacen("{}", &bytes_copied.to_string(), 1)
            .replacen("{}", &total_bytes.to_string(), 1)
    }

    fn succeeded(self, cx: &App, entries_copied: usize) -> String {
        let template = match self {
            Self::Copy => tr(
                cx,
                "terminal_mode.copy.succeeded",
                "Copied {} item(s) to the target session",
            ),
            Self::Move => tr(
                cx,
                "terminal_mode.move.succeeded",
                "Moved {} item(s) to the target session",
            ),
            Self::Download => tr(
                cx,
                "terminal_mode.download.succeeded",
                "Downloaded {} item(s)",
            ),
            Self::Upload => tr(cx, "terminal_mode.upload.succeeded", "Uploaded {} item(s)"),
        };
        template.replacen("{}", &entries_copied.to_string(), 1)
    }

    fn failed(self, cx: &App, error: &anyhow::Error) -> String {
        let template = match self {
            Self::Copy => tr(
                cx,
                "terminal_mode.copy.failed",
                "Copy to session failed: {}",
            ),
            Self::Move => tr(
                cx,
                "terminal_mode.move.failed",
                "Move to session failed: {}",
            ),
            Self::Download => tr(cx, "terminal_mode.download.failed", "Download failed: {}"),
            Self::Upload => tr(cx, "terminal_mode.upload.failed", "Upload failed: {}"),
        };
        template.replacen("{}", &format!("{error:#}"), 1)
    }
}

/// Builds a progress callback that throttles updates to whole percentages.
fn transfer_progress_channel() -> (
    vfs::VfsTransferProgressCallback,
    UnboundedReceiver<vfs::VfsTransferProgress>,
) {
    let (progress_sender, progress_receiver) =
        futures::channel::mpsc::unbounded::<vfs::VfsTransferProgress>();
    let last_percent = Arc::new(AtomicU64::new(u64::MAX));
    let progress_callback: vfs::VfsTransferProgressCallback = Arc::new(move |progress| {
        let percent = progress
            .bytes_copied
            .saturating_mul(100)
            .checked_div(progress.total_bytes)
            .unwrap_or(0);
        if last_percent.swap(percent, Ordering::Relaxed) != percent {
            progress_sender.unbounded_send(progress).ok();
        }
    });
    (progress_callback, progress_receiver)
}

/// Right panel: the active session's file tree on top and a preview pane below
/// (R4/R5).
pub struct SessionFilePanel {
    store: WeakEntity<SessionStore>,
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    tree: Option<Entity<ProjectPanel>>,
    preview: Option<Entity<Pane>>,
    active_session: Option<SessionId>,
    root: SessionRoot,
}

impl SessionFilePanel {
    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> anyhow::Result<Entity<Self>> {
        workspace.update_in(&mut cx, |workspace, window, cx| {
            Self::new(workspace, window, cx)
        })
    }

    fn new(
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        let store = SessionStore::for_workspace(workspace, cx);
        let workspace_handle = workspace.weak_handle();
        let panel = cx.new(|cx| Self {
            store: store.downgrade(),
            workspace: workspace_handle,
            focus_handle: cx.focus_handle(),
            tree: None,
            preview: None,
            active_session: None,
            root: SessionRoot::Home,
        });

        panel.update(cx, |_panel, cx| {
            cx.subscribe_in(
                &store,
                window,
                |panel, store, event, window, cx| match event {
                    SessionEvent::SessionActivated(id) => {
                        panel.on_session_activated(*id, window, cx);
                    }
                    SessionEvent::SessionsChanged => {
                        let active = store.read(cx).active_session;
                        if active != panel.active_session {
                            match active {
                                Some(id) => panel.on_session_activated(id, window, cx),
                                None => panel.clear(cx),
                            }
                        }
                        cx.notify();
                    }
                    SessionEvent::MetricsUpdated(_) => cx.notify(),
                },
            )
            .detach();
        });

        panel
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        self.tree = None;
        self.preview = None;
        self.active_session = None;
        cx.notify();
    }

    fn on_session_activated(&mut self, id: SessionId, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_session == Some(id) {
            return;
        }
        let Some(store) = self.store.upgrade() else {
            return;
        };
        let Some(project) = store
            .read(cx)
            .session(id)
            .map(|session| session.project.clone())
        else {
            return;
        };
        self.active_session = Some(id);

        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let preview_enabled = crate::TerminalModeSettings::get_global(cx).preview_enabled;
        cx.spawn_in(window, async move |panel, cx| {
            let (tree, preview, open_target) =
                workspace.update_in(cx, |workspace, window, cx| {
                    let tree = ProjectPanel::new(project.clone(), workspace, window, cx);
                    if preview_enabled {
                        let preview = workspace.create_pane_for_project(project, window, cx);
                        let open_target = preview.downgrade();
                        (tree, Some(preview), open_target)
                    } else {
                        (tree, None, workspace.active_pane().downgrade())
                    }
                })?;
            tree.update(cx, |tree, cx| tree.set_open_target(open_target, cx));
            panel
                .update(cx, |panel, cx| {
                    if panel.active_session != Some(id) {
                        return;
                    }
                    panel.tree = Some(tree);
                    panel.preview = preview;
                    cx.notify();
                })
                .log_err();
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    /// Resolves the session root and worktree that own `project_path`.
    ///
    /// The tree can display more than one root at once (the session's home and
    /// the remote filesystem), so transfers key off the selected path's
    /// worktree instead of the toolbar's root hint; otherwise a copy would
    /// resolve a filesystem-relative path against the home worktree.
    fn root_and_worktree_for(
        &self,
        session_id: SessionId,
        project_path: &ProjectPath,
        cx: &App,
    ) -> Option<(SessionRoot, Entity<Worktree>)> {
        let store = self.store.upgrade()?;
        let store = store.read(cx);
        for root in [SessionRoot::Home, SessionRoot::Filesystem] {
            if let Some(worktree) = store.worktree_for_root(session_id, root)
                && worktree.read(cx).id() == project_path.worktree_id
            {
                return Some((root, worktree));
            }
        }
        None
    }

    /// Resolves the worktree backing the toolbar's selected root, used when the
    /// tree has no selection to key off.
    fn root_worktree(
        &self,
        session_id: SessionId,
        cx: &App,
    ) -> Option<(SessionRoot, Entity<Worktree>)> {
        let store = self.store.upgrade()?;
        let worktree = store.read(cx).worktree_for_root(session_id, self.root)?;
        Some((self.root, worktree))
    }

    /// Switches the file tree's root, mounting the filesystem root on demand.
    fn set_root(&mut self, root: SessionRoot, cx: &mut Context<Self>) {
        if self.root == root {
            return;
        }
        self.root = root;
        if root == SessionRoot::Filesystem
            && let Some(id) = self.active_session
            && let Some(store) = self.store.upgrade()
        {
            let mount = store.update(cx, |store, cx| store.ensure_filesystem_worktree(id, cx));
            cx.spawn(async move |panel, cx| {
                let outcome = mount.await;
                panel.update(cx, |_panel, cx| cx.notify()).log_err();
                outcome.map(|_| ())
            })
            .detach_and_log_err(cx);
        } else {
            cx.notify();
        }
    }

    /// Streams throttled transfer progress into a toast.
    fn spawn_transfer_progress(
        &self,
        toast_id: NotificationId,
        operation: TransferOperation,
        mut progress_receiver: UnboundedReceiver<vfs::VfsTransferProgress>,
        cx: &mut Context<Self>,
    ) {
        let workspace = self.workspace.clone();
        cx.spawn(async move |_panel, cx| {
            while let Some(progress) = progress_receiver.next().await {
                workspace
                    .update(cx, |workspace, cx| {
                        let message =
                            operation.progress(cx, progress.bytes_copied, progress.total_bytes);
                        workspace.show_toast(Toast::new(toast_id.clone(), message), cx);
                    })
                    .log_err();
            }
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    fn show_in_progress_toast(
        &self,
        toast_id: &NotificationId,
        operation: TransferOperation,
        cx: &mut Context<Self>,
    ) {
        if let Some(workspace) = self.workspace.upgrade() {
            workspace.update(cx, |workspace, cx| {
                workspace.show_toast(Toast::new(toast_id.clone(), operation.in_progress(cx)), cx);
            });
        }
    }

    /// Copies the tree's selected entry to the local filesystem (R7).
    fn download_selected(&mut self, cx: &mut Context<Self>) {
        let Some(session_id) = self.active_session else {
            return;
        };
        let Some(project_path) = self
            .tree
            .as_ref()
            .and_then(|tree| tree.read(cx).selected_path(cx))
        else {
            return;
        };
        let Some((root, worktree)) = self.root_and_worktree_for(session_id, &project_path, cx)
        else {
            return;
        };
        let Some(store) = self.store.upgrade() else {
            return;
        };
        let manager = store.read(cx).transfer_manager.clone();
        let Some(fs) = self
            .workspace
            .upgrade()
            .map(|workspace| workspace.read(cx).app_state().fs.clone())
        else {
            return;
        };
        let mount_id = mount_id_for_session_root(session_id.0, root);
        let relative_path = project_path.path;
        let destination = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr(cx, "terminal_mode.download.prompt", "Download").into()),
        });
        let toast_id = NotificationId::unique::<SessionFilePanel>();
        let workspace = self.workspace.clone();

        let (progress_callback, progress_receiver) = transfer_progress_channel();
        self.spawn_transfer_progress(
            toast_id.clone(),
            TransferOperation::Download,
            progress_receiver,
            cx,
        );

        cx.spawn(async move |_panel, cx| {
            let directory = match destination.await {
                Ok(Ok(Some(mut paths))) => paths.pop(),
                _ => None,
            };
            let Some(directory) = directory else {
                return anyhow::Ok(());
            };
            workspace
                .update(cx, |workspace, cx| {
                    let message = TransferOperation::Download.in_progress(cx);
                    workspace.show_toast(Toast::new(toast_id.clone(), message), cx);
                })
                .log_err();
            let outcome = download_from_session(
                &manager,
                fs,
                &worktree,
                mount_id,
                &relative_path,
                &directory,
                Some(progress_callback),
                cx,
            )
            .await;
            workspace
                .update(cx, |workspace, cx| {
                    let message = match &outcome {
                        Ok(outcome) => {
                            TransferOperation::Download.succeeded(cx, outcome.entries_copied)
                        }
                        Err(error) => TransferOperation::Download.failed(cx, error),
                    };
                    workspace.show_toast(Toast::new(toast_id, message), cx);
                })
                .log_err();
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    /// Uploads local files into the tree's selected directory (R7).
    fn upload_files(&mut self, cx: &mut Context<Self>) {
        let Some(session_id) = self.active_session else {
            return;
        };
        let selected_path = self
            .tree
            .as_ref()
            .and_then(|tree| tree.read(cx).selected_path(cx));
        let resolved = match selected_path.as_ref() {
            Some(project_path) => self.root_and_worktree_for(session_id, project_path, cx),
            None => self.root_worktree(session_id, cx),
        };
        let Some((root, worktree)) = resolved else {
            return;
        };
        let target_relative_directory = selected_path
            .as_ref()
            .map_or_else(RelPath::empty_arc, |project_path| {
                self.upload_target_directory(&worktree, project_path, cx)
            });
        let Some(store) = self.store.upgrade() else {
            return;
        };
        let manager = store.read(cx).transfer_manager.clone();
        let Some(fs) = self
            .workspace
            .upgrade()
            .map(|workspace| workspace.read(cx).app_state().fs.clone())
        else {
            return;
        };
        let mount_id = mount_id_for_session_root(session_id.0, root);
        let source_paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(tr(cx, "terminal_mode.upload.prompt", "Upload").into()),
        });
        let toast_id = NotificationId::unique::<SessionFilePanel>();
        let workspace = self.workspace.clone();

        let (progress_callback, progress_receiver) = transfer_progress_channel();
        self.spawn_transfer_progress(
            toast_id.clone(),
            TransferOperation::Upload,
            progress_receiver,
            cx,
        );

        cx.spawn(async move |_panel, cx| {
            let paths: Vec<PathBuf> = match source_paths.await {
                Ok(Ok(Some(paths))) => paths,
                _ => return anyhow::Ok(()),
            };
            if paths.is_empty() {
                return anyhow::Ok(());
            }
            workspace
                .update(cx, |workspace, cx| {
                    let message = TransferOperation::Upload.in_progress(cx);
                    workspace.show_toast(Toast::new(toast_id.clone(), message), cx);
                })
                .log_err();
            let outcome = upload_to_session(
                &manager,
                fs,
                &worktree,
                mount_id,
                &paths,
                &target_relative_directory,
                Some(progress_callback),
                cx,
            )
            .await;
            workspace
                .update(cx, |workspace, cx| {
                    let message = match &outcome {
                        Ok(outcomes) => {
                            let entries = outcomes.iter().map(|o| o.entries_copied).sum();
                            TransferOperation::Upload.succeeded(cx, entries)
                        }
                        Err(error) => TransferOperation::Upload.failed(cx, error),
                    };
                    workspace.show_toast(Toast::new(toast_id, message), cx);
                })
                .log_err();
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    /// The directory uploads should land in: the selected directory, its
    /// parent when a file is selected, or the worktree root.
    fn upload_target_directory(
        &self,
        worktree: &Entity<Worktree>,
        project_path: &ProjectPath,
        cx: &App,
    ) -> Arc<RelPath> {
        let is_directory = worktree
            .read(cx)
            .entry_for_path(project_path.path.as_ref())
            .is_some_and(|entry| entry.is_dir());
        if is_directory {
            project_path.path.clone()
        } else {
            project_path
                .path
                .parent()
                .map_or_else(RelPath::empty_arc, Arc::from)
        }
    }

    /// Copies or moves the tree's selected entry into `target_id`'s home
    /// directory, relaying through the local process (R6).
    fn transfer_selected_to(
        &mut self,
        target_id: SessionId,
        move_source: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(source_id) = self.active_session else {
            return;
        };
        let Some(source_path) = self
            .tree
            .as_ref()
            .and_then(|tree| tree.read(cx).selected_path(cx))
        else {
            return;
        };
        let Some((source_root, source_worktree)) =
            self.root_and_worktree_for(source_id, &source_path, cx)
        else {
            return;
        };
        let Some(store) = self.store.upgrade() else {
            return;
        };
        let (target_worktree, manager) = {
            let store = store.read(cx);
            let Some(target_worktree) = store
                .session(target_id)
                .and_then(|session| session.home_worktree.clone())
            else {
                return;
            };
            (target_worktree, store.transfer_manager.clone())
        };

        let source_mount_id = mount_id_for_session_root(source_id.0, source_root);
        let target_mount_id = mount_id_for_session(target_id.0);
        let relative_path = source_path.path;
        let workspace = self.workspace.clone();
        let toast_id = NotificationId::unique::<SessionFilePanel>();
        let operation = if move_source {
            TransferOperation::Move
        } else {
            TransferOperation::Copy
        };

        self.show_in_progress_toast(&toast_id, operation, cx);
        let (progress_callback, progress_receiver) = transfer_progress_channel();
        self.spawn_transfer_progress(toast_id.clone(), operation, progress_receiver, cx);

        cx.spawn(async move |_panel, cx| {
            let outcome = if move_source {
                move_between_sessions(
                    &manager,
                    &source_worktree,
                    source_mount_id,
                    &relative_path,
                    &target_worktree,
                    target_mount_id,
                    Some(progress_callback),
                    cx,
                )
                .await
            } else {
                copy_between_sessions(
                    &manager,
                    &source_worktree,
                    source_mount_id,
                    &relative_path,
                    &target_worktree,
                    target_mount_id,
                    Some(progress_callback),
                    cx,
                )
                .await
            };

            workspace
                .update(cx, |workspace, cx| {
                    let message = match &outcome {
                        Ok(outcome) => operation.succeeded(cx, outcome.entries_copied),
                        Err(error) => operation.failed(cx, error),
                    };
                    workspace.show_toast(Toast::new(toast_id, message), cx);
                })
                .log_err();

            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }
}

impl Focusable for SessionFilePanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for SessionFilePanel {}

impl Render for SessionFilePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut column = v_flex().size_full().overflow_hidden();

        let monitor_visible = self
            .store
            .upgrade()
            .is_some_and(|store| store.read(cx).monitor_visible);
        let metrics = self.active_session.and_then(|id| {
            self.store
                .upgrade()
                .and_then(|store| store.read(cx).metrics(id).cloned())
        });
        let monitor_enabled = crate::TerminalModeSettings::get_global(cx).monitor_enabled;

        let other_sessions: Vec<(SessionId, SharedString)> = self
            .store
            .upgrade()
            .map(|store| {
                let store = store.read(cx);
                store
                    .sessions
                    .iter()
                    .filter(|session| Some(session.id) != self.active_session)
                    .map(|session| {
                        (
                            session.id,
                            SharedString::from(session.connection_options.display_name()),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut toolbar = h_flex().flex_wrap().p_1().gap_1();
        let mut has_toolbar = false;

        if self.active_session.is_some() {
            has_toolbar = true;
            let (root_key, root_fallback) = self.root.label_key();
            let root_menu = PopoverMenu::new("session-file-root-menu")
                .trigger(Button::new(
                    "session-file-root-button",
                    tr(cx, root_key, root_fallback),
                ))
                .menu({
                    let panel_handle = cx.entity().downgrade();
                    move |window, cx| {
                        let panel_handle = panel_handle.clone();
                        Some(ContextMenu::build(window, cx, move |menu, _window, cx| {
                            let mut menu = menu;
                            for candidate in [SessionRoot::Home, SessionRoot::Filesystem] {
                                let panel_handle = panel_handle.clone();
                                let (key, fallback) = candidate.label_key();
                                menu =
                                    menu.entry(tr(cx, key, fallback), None, move |_window, cx| {
                                        panel_handle
                                            .update(cx, |panel, cx| panel.set_root(candidate, cx))
                                            .log_err();
                                    });
                            }
                            menu
                        }))
                    }
                })
                .into_any_element();
            toolbar = toolbar
                .child(root_menu)
                .child(
                    Button::new(
                        "session-file-download",
                        tr(cx, "terminal_mode.download.action", "Download…"),
                    )
                    .tooltip(Tooltip::text(tr(
                        cx,
                        "terminal_mode.download.tooltip",
                        "Download the selected items to this machine",
                    )))
                    .on_click(
                        cx.listener(|panel, _event, _window, cx| panel.download_selected(cx)),
                    ),
                )
                .child(
                    Button::new(
                        "session-file-upload",
                        tr(cx, "terminal_mode.upload.action", "Upload…"),
                    )
                    .tooltip(Tooltip::text(tr(
                        cx,
                        "terminal_mode.upload.tooltip",
                        "Upload files from this machine to the session",
                    )))
                    .on_click(cx.listener(|panel, _event, _window, cx| panel.upload_files(cx))),
                )
                .child(
                    Button::new(
                        "session-file-monitor-toggle",
                        if monitor_visible {
                            tr(cx, "terminal_mode.monitor.hide", "Hide Monitor")
                        } else {
                            tr(cx, "terminal_mode.monitor.show", "Show Monitor")
                        },
                    )
                    .tooltip(Tooltip::text(tr(
                        cx,
                        "terminal_mode.monitor.tooltip",
                        "Toggle live CPU, memory, disk and network usage for the session",
                    )))
                    .on_click(cx.listener(|panel, _event, _window, cx| {
                        if let Some(store) = panel.store.upgrade() {
                            store.update(cx, |store, cx| {
                                let visible = !store.monitor_visible;
                                store.set_monitor_visible(visible, cx);
                            });
                        }
                    })),
                );
        }

        if !other_sessions.is_empty() {
            has_toolbar = true;
            let panel_handle = cx.entity().downgrade();
            let menu_sessions = other_sessions;
            let copy_menu = PopoverMenu::new("session-file-copy-menu")
                .trigger(Button::new(
                    "session-file-copy-button",
                    tr(cx, "terminal_mode.copy.action", "Copy to session…"),
                ))
                .menu({
                    let panel_handle = panel_handle.clone();
                    let entries = menu_sessions.clone();
                    move |window, cx| {
                        let panel_handle = panel_handle.clone();
                        let entries = entries.clone();
                        Some(ContextMenu::build(window, cx, move |menu, _window, _cx| {
                            let mut menu = menu;
                            for (id, name) in entries {
                                let panel_handle = panel_handle.clone();
                                menu = menu.entry(name, None, move |_window, cx| {
                                    panel_handle
                                        .update(cx, |panel, cx| {
                                            panel.transfer_selected_to(id, false, cx)
                                        })
                                        .log_err();
                                });
                            }
                            menu
                        }))
                    }
                })
                .into_any_element();
            let move_menu = PopoverMenu::new("session-file-move-menu")
                .trigger(Button::new(
                    "session-file-move-button",
                    tr(cx, "terminal_mode.move.action", "Move to session…"),
                ))
                .menu(move |window, cx| {
                    let panel_handle = panel_handle.clone();
                    let entries = menu_sessions.clone();
                    Some(ContextMenu::build(window, cx, move |menu, _window, _cx| {
                        let mut menu = menu;
                        for (id, name) in entries {
                            let panel_handle = panel_handle.clone();
                            menu = menu.entry(name, None, move |_window, cx| {
                                panel_handle
                                    .update(cx, |panel, cx| {
                                        panel.transfer_selected_to(id, true, cx)
                                    })
                                    .log_err();
                            });
                        }
                        menu
                    }))
                })
                .into_any_element();
            toolbar = toolbar.child(copy_menu).child(move_menu);
        }

        if has_toolbar {
            column = column.child(toolbar);
        }

        match (&self.tree, &self.preview) {
            (Some(tree), Some(preview)) => {
                column = column
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .child(tree.clone()),
                    )
                    .when(
                        crate::TerminalModeSettings::get_global(cx).preview_enabled,
                        |column| {
                            column.child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_hidden()
                                    .border_t_1()
                                    .border_color(cx.theme().colors().border)
                                    .child(preview.clone()),
                            )
                        },
                    );
            }
            (Some(tree), None) => {
                column = column.child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .overflow_hidden()
                        .child(tree.clone()),
                );
            }
            _ => {
                column = column.child(
                    v_flex()
                        .size_full()
                        .items_center()
                        .justify_center()
                        .gap_2()
                        .child(Label::new(tr(
                            cx,
                            "terminal_mode.file_panel.empty",
                            "No active session",
                        )))
                        .child(
                            Button::new(
                                "session-file-connect",
                                tr(
                                    cx,
                                    "terminal_mode.file_panel.connect",
                                    "Connect to a Server…",
                                ),
                            )
                            .on_click(cx.listener(
                                |panel, _event, window, cx| {
                                    let Some(workspace) = panel.workspace.upgrade() else {
                                        return;
                                    };
                                    workspace.update(cx, |workspace, cx| {
                                        workspace.toggle_panel_focus::<crate::SshSessionPanel>(
                                            window, cx,
                                        );
                                    });
                                },
                            )),
                        ),
                );
            }
        }

        if monitor_visible && self.active_session.is_some() {
            column = column.child(
                div()
                    .id("terminal-mode-monitor-scroll")
                    .flex_shrink_0()
                    .max_h(px(260.))
                    .overflow_y_scroll()
                    .child(render_monitor(metrics.as_ref(), monitor_enabled, cx)),
            );
        }

        column
    }
}

impl Panel for SessionFilePanel {
    fn persistent_name() -> &'static str {
        "Session Files"
    }

    fn panel_key() -> &'static str {
        SESSION_FILE_PANEL_KEY
    }

    fn position(&self, _window: &Window, _cx: &App) -> DockPosition {
        DockPosition::Right
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        position == DockPosition::Right
    }

    fn set_position(
        &mut self,
        _position: DockPosition,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    fn default_size(&self, _window: &Window, cx: &App) -> Pixels {
        crate::TerminalModeSettings::get_global(cx).file_panel_default_width
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<IconName> {
        Some(IconName::FileTree)
    }

    fn icon_tooltip(&self, _window: &Window, cx: &App) -> Option<SharedString> {
        Some(tr(cx, "terminal_mode.file_panel.title", "Session Files").into())
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFilePanel)
    }

    fn activation_priority(&self) -> u32 {
        9
    }
}
