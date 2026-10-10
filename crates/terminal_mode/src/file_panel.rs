use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ::settings::Settings as _;
use futures::StreamExt as _;
use gpui::{
    Action, App, AppContext as _, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle,
    Focusable, IntoElement, ParentElement, Pixels, Render, SharedString, Styled, WeakEntity,
    Window, div,
};
use i18n::tr;
use project_panel::ProjectPanel;
use ui::{Button, ContextMenu, IconName, Label, PopoverMenu, prelude::*};
use util::ResultExt as _;
use workspace::{
    Pane, Toast, Workspace,
    dock::{DockPosition, Panel, PanelEvent},
    notifications::NotificationId,
};

use crate::ToggleFilePanel;
use crate::session::{Event as SessionEvent, SessionId, SessionStore};
use crate::transfer::{copy_between_sessions, mount_id_for_session};

const SESSION_FILE_PANEL_KEY: &str = "SessionFilePanel";

/// Right panel: the active session's file tree on top and a preview pane below
/// (R4/R5).
pub struct SessionFilePanel {
    store: WeakEntity<SessionStore>,
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    tree: Option<Entity<ProjectPanel>>,
    preview: Option<Entity<Pane>>,
    active_session: Option<SessionId>,
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
                        if active == panel.active_session {
                            return;
                        }
                        match active {
                            Some(id) => panel.on_session_activated(id, window, cx),
                            None => panel.clear(cx),
                        }
                    }
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

    /// Copies the tree's selected entry into `target_id`'s home directory,
    /// relaying through the local process (R6).
    fn copy_selected_to(&mut self, target_id: SessionId, cx: &mut Context<Self>) {
        let Some(source_id) = self.active_session else {
            return;
        };
        let Some(store) = self.store.upgrade() else {
            return;
        };
        let Some(source_path) = self
            .tree
            .as_ref()
            .and_then(|tree| tree.read(cx).selected_path(cx))
        else {
            return;
        };

        let (source_worktree, target_worktree, manager) = {
            let store = store.read(cx);
            let Some(source) = store.session(source_id) else {
                return;
            };
            let Some(target) = store.session(target_id) else {
                return;
            };
            let (Some(source_worktree), Some(target_worktree)) =
                (source.home_worktree.clone(), target.home_worktree.clone())
            else {
                return;
            };
            (
                source_worktree,
                target_worktree,
                store.transfer_manager.clone(),
            )
        };

        let source_mount_id = mount_id_for_session(source_id.0);
        let target_mount_id = mount_id_for_session(target_id.0);
        let relative_path = source_path.path;
        let workspace = self.workspace.clone();
        let toast_id = NotificationId::unique::<SessionFilePanel>();

        if let Some(workspace) = self.workspace.upgrade() {
            workspace.update(cx, |workspace, cx| {
                workspace.show_toast(
                    Toast::new(
                        toast_id.clone(),
                        tr(cx, "terminal_mode.copy.in_progress", "Copying…"),
                    ),
                    cx,
                );
            });
        }

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

        let workspace_for_progress = self.workspace.clone();
        let toast_id_for_progress = toast_id.clone();
        cx.spawn(async move |_panel, cx| {
            let mut progress_receiver = progress_receiver;
            while let Some(progress) = progress_receiver.next().await {
                workspace_for_progress
                    .update(cx, |workspace, cx| {
                        let message =
                            tr(cx, "terminal_mode.copy.progress", "Copying {} of {} bytes…")
                                .replacen("{}", &progress.bytes_copied.to_string(), 1)
                                .replacen("{}", &progress.total_bytes.to_string(), 1);
                        workspace
                            .show_toast(Toast::new(toast_id_for_progress.clone(), message), cx);
                    })
                    .log_err();
            }
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);

        cx.spawn(async move |_panel, cx| {
            let outcome = copy_between_sessions(
                &manager,
                &source_worktree,
                source_mount_id,
                &relative_path,
                &target_worktree,
                target_mount_id,
                Some(progress_callback),
                cx,
            )
            .await;

            workspace
                .update(cx, |workspace, cx| {
                    let message = match &outcome {
                        Ok(outcome) => tr(
                            cx,
                            "terminal_mode.copy.succeeded",
                            "Copied {} item(s) to the target session",
                        )
                        .replacen(
                            "{}",
                            &outcome.entries_copied.to_string(),
                            1,
                        ),
                        Err(error) => tr(
                            cx,
                            "terminal_mode.copy.failed",
                            "Copy to session failed: {}",
                        )
                        .replacen("{}", &format!("{error:#}"), 1),
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

        if !other_sessions.is_empty() {
            let panel_handle = cx.entity().downgrade();
            let menu_sessions = other_sessions;
            let copy_menu = PopoverMenu::new("session-file-copy-menu")
                .trigger(Button::new(
                    "session-file-copy-button",
                    tr(cx, "terminal_mode.copy.action", "Copy to session…"),
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
                                    .update(cx, |panel, cx| panel.copy_selected_to(id, cx))
                                    .log_err();
                            });
                        }
                        menu
                    }))
                })
                .into_any_element();
            column = column.child(div().p_1().child(copy_menu));
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
                column = column.child(Label::new(tr(
                    cx,
                    "terminal_mode.file_panel.empty",
                    "No active session",
                )));
            }
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
