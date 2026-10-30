use std::collections::BTreeSet;
use std::sync::Arc;

use ::settings::{Settings as _, SettingsStore, watch_config_file};
use editor::{Editor, EditorEvent};
use fs::Fs;
use futures::StreamExt as _;
use gpui::{
    Action, Anchor, App, AppContext as _, AsyncWindowContext, ClickEvent, Context, DismissEvent,
    Entity, EventEmitter, FocusHandle, Focusable, Hsla, InteractiveElement, IntoElement,
    KeyContext, MouseDownEvent, ParentElement, Pixels, Point, Render, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Task, WeakEntity, Window, anchored, deferred,
    px,
};
use i18n::tr;
use paths::{global_ssh_config_file, user_ssh_config_file};
use recent_projects::RemoteSettings;
use recent_projects::ssh_config::parse_ssh_config_hosts;
use remote::{RemoteConnectionOptions, WslConnectionOptions};
use ui::{
    Button, Color, ContextMenu, Icon, IconName, IconSize, Label, LabelSize, ListItem, PopoverMenu,
    Tooltip, prelude::*,
};
use util::ResultExt as _;
use workspace::{
    Toast, Workspace,
    dock::{DockPosition, Panel, PanelEvent},
    notifications::NotificationId,
};

use crate::session::{SessionId, SessionStatus, SessionStore, is_connection_cancelled};
use crate::{
    ConfirmConnection, DisconnectSession, ReconnectSession, SelectNextConnection,
    SelectPreviousConnection, ToggleSessionPanel,
};

pub(crate) const SSH_SESSION_PANEL_KEY: &str = "SshSessionPanel";

#[derive(Clone)]
struct ConnectionEntry {
    name: SharedString,
    detail: SharedString,
    options: RemoteConnectionOptions,
}

/// Left panel listing saved connections and `~/.ssh/config` hosts (R1).
pub struct SshSessionPanel {
    store: WeakEntity<SessionStore>,
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    filter_editor: Entity<Editor>,
    filter_query: String,
    ssh_config_hosts: BTreeSet<SharedString>,
    ssh_config_watchers: Vec<Task<()>>,
    selected_index: Option<usize>,
    context_menu: Option<(Entity<ContextMenu>, Point<Pixels>, Subscription)>,
}

impl SshSessionPanel {
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
        let fs = workspace.app_state().fs.clone();
        let filter_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(
                &tr(
                    cx,
                    "terminal_mode.session_panel.search",
                    "Search connections…",
                ),
                window,
                cx,
            );
            editor
        });
        let panel = cx.new(|cx| Self {
            store: store.downgrade(),
            workspace: workspace_handle,
            focus_handle: cx.focus_handle(),
            filter_editor,
            filter_query: String::new(),
            ssh_config_hosts: BTreeSet::new(),
            ssh_config_watchers: Vec::new(),
            selected_index: None,
            context_menu: None,
        });

        let panel_handle = panel.downgrade();
        panel.update(cx, |panel, cx| {
            let filter_editor = panel.filter_editor.clone();
            cx.subscribe_in(
                &filter_editor,
                window,
                |panel, _editor, event: &EditorEvent, _window, cx| {
                    if matches!(event, EditorEvent::BufferEdited) {
                        panel.filter_query = panel.filter_editor.read(cx).text(cx);
                        cx.notify();
                    }
                },
            )
            .detach();

            cx.subscribe_in(&store, window, |_panel, _store, _event, _window, cx| {
                cx.notify();
            })
            .detach();

            cx.observe_global_in::<SettingsStore>(window, |_panel, _window, cx| {
                cx.notify();
            })
            .detach();

            panel.ssh_config_watchers =
                watch_ssh_configs(fs.clone(), panel_handle.clone(), window, cx);
        });

        let store_handle = store.downgrade();
        cx.defer_in(window, move |_workspace, window, cx| {
            if let Some(store) = store_handle.upgrade() {
                store.update(cx, |store, cx| store.restore(window, cx));
            }
        });

        panel
    }

    fn entries(&self, cx: &App) -> Vec<ConnectionEntry> {
        let settings = RemoteSettings::get_global(cx);
        let mut entries = Vec::new();
        let mut saved_hosts = BTreeSet::new();

        for connection in settings.ssh_connections() {
            saved_hosts.insert(connection.host.clone());
            let options = settings.connection_options_for(
                connection.host.clone(),
                connection.port,
                connection.username.clone(),
            );
            let options = RemoteConnectionOptions::Ssh(options);
            let name = options.display_name();
            entries.push(ConnectionEntry {
                name: name.into(),
                detail: connection.host.clone().into(),
                options,
            });
        }

        for connection in settings.wsl_connections() {
            let options = RemoteConnectionOptions::Wsl(WslConnectionOptions::from(connection));
            entries.push(ConnectionEntry {
                name: options.display_name().into(),
                detail: tr(cx, "terminal_mode.session_panel.wsl", "WSL").into(),
                options,
            });
        }

        if settings.read_ssh_config {
            for host in &self.ssh_config_hosts {
                if saved_hosts.contains(host.as_ref()) {
                    continue;
                }
                let options = settings.connection_options_for(host.to_string(), None, None);
                let options = RemoteConnectionOptions::Ssh(options);
                entries.push(ConnectionEntry {
                    name: host.clone(),
                    detail: tr(
                        cx,
                        "terminal_mode.session_panel.from_ssh_config",
                        "~/.ssh/config",
                    )
                    .into(),
                    options,
                });
            }
        }

        if self.filter_query.is_empty() {
            return entries;
        }

        let query = self.filter_query.to_lowercase();
        entries
            .into_iter()
            .filter(|entry| {
                entry.name.to_lowercase().contains(&query)
                    || entry.detail.to_lowercase().contains(&query)
            })
            .collect()
    }

    fn active_session(&self, cx: &App) -> Option<SessionId> {
        self.store
            .upgrade()
            .and_then(|store| store.read(cx).active_session)
    }

    /// Resolves the status shown next to an entry: the status of a live session
    /// with the same options, `Connecting` while a connection is pending, or
    /// `None` when the entry has never been opened.
    fn status_for_options(
        &self,
        options: &RemoteConnectionOptions,
        cx: &App,
    ) -> Option<SessionStatus> {
        let store = self.store.upgrade()?;
        let store = store.read(cx);
        if let Some(session) = store
            .sessions
            .iter()
            .find(|session| &session.connection_options == options)
        {
            return Some(session.status);
        }
        store
            .pending_connections
            .iter()
            .any(|pending| &pending.connection_options == options)
            .then_some(SessionStatus::Connecting)
    }

    /// Opens the shared remote-server manager so connections are added, edited
    /// and removed through the same interaction used everywhere else.
    fn open_new_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        workspace.update(cx, |workspace, cx| {
            let fs = workspace.project().read(cx).fs().clone();
            let workspace_handle = cx.entity().downgrade();
            workspace.toggle_modal(window, cx, |window, cx| {
                recent_projects::RemoteServerProjects::new(false, fs, window, workspace_handle, cx)
            });
        });
    }

    /// Opens the broadcast form that sends one command to every session.
    fn open_broadcast_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(store) = self.store.upgrade() else {
            return;
        };
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        workspace.update(cx, |workspace, cx| {
            workspace.toggle_modal(window, cx, |window, cx| {
                crate::BroadcastModal::new(store, window, cx)
            });
        });
    }

    fn connect(
        &mut self,
        options: RemoteConnectionOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(store) = self.store.upgrade() else {
            return;
        };
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let task = store.update(cx, |store, cx| store.connect(options, window, cx));
        cx.spawn_in(window, async move |_panel, cx| {
            if let Err(error) = task.await
                && !is_connection_cancelled(&error)
            {
                workspace
                    .update_in(cx, |workspace, _window, cx| {
                        workspace.show_toast(
                            Toast::new(
                                NotificationId::unique::<SshSessionPanel>(),
                                tr(cx, "terminal_mode.connect.failed", "Connection failed: {}")
                                    .replacen("{}", &format!("{error:#}"), 1),
                            ),
                            cx,
                        );
                    })
                    .log_err();
            }
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    /// The key context for the panel, switching to `editing` while the filter
    /// editor holds focus so list navigation does not hijack the caret keys.
    fn dispatch_context(&self, window: &mut Window, cx: &mut Context<Self>) -> KeyContext {
        let mut context = KeyContext::new_with_defaults();
        context.add("SshSessionPanel");
        context.add("menu");
        let identifier = if self.filter_editor.focus_handle(cx).is_focused(window) {
            "editing"
        } else {
            "not_editing"
        };
        context.add(identifier);
        context
    }

    /// Moves the keyboard selection by `delta` rows, clamped to the list ends.
    fn move_selection(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.entries(cx).len();
        if count == 0 {
            return;
        }
        let next = match self.selected_index {
            Some(current) => (current as isize + delta).clamp(0, count as isize - 1) as usize,
            None if delta > 0 => 0,
            None => count - 1,
        };
        self.selected_index = Some(next);
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn select_next(
        &mut self,
        _: &SelectNextConnection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_selection(1, window, cx);
    }

    fn select_previous(
        &mut self,
        _: &SelectPreviousConnection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_selection(-1, window, cx);
    }

    fn confirm(&mut self, _: &ConfirmConnection, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.selected_index else {
            return;
        };
        self.activate_entry(index, window, cx);
    }

    /// Connects the entry at `index`, or focuses its session when already live.
    fn activate_entry(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.entries(cx).get(index).cloned() else {
            return;
        };
        self.selected_index = Some(index);
        let session_id = self.session_id_for_options(&entry.options, cx);
        if let Some(session_id) = session_id {
            if let Some(store) = self.store.upgrade() {
                store.update(cx, |store, cx| store.set_active_session(session_id, cx));
            }
        } else if self.status_for_options(&entry.options, cx) != Some(SessionStatus::Connecting) {
            self.connect(entry.options, window, cx);
        }
        cx.notify();
    }

    fn session_id_for_options(
        &self,
        options: &RemoteConnectionOptions,
        cx: &App,
    ) -> Option<SessionId> {
        self.store.upgrade().and_then(|store| {
            store
                .read(cx)
                .sessions
                .iter()
                .find(|session| &session.connection_options == options)
                .map(|session| session.id)
        })
    }

    /// Deploys the row context menu for the connection at `index`.
    fn deploy_context_menu(
        &mut self,
        position: Point<Pixels>,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.entries(cx).get(index).cloned() else {
            return;
        };
        self.selected_index = Some(index);

        let session_id = self.session_id_for_options(&entry.options, cx);
        let focus_handle = self.focus_handle.clone();
        let panel_handle = cx.entity().downgrade();
        let options = entry.options;

        let context_menu = ContextMenu::build(window, cx, move |menu, _, cx| {
            let mut menu = menu.context(focus_handle);
            if let Some(session_id) = session_id {
                menu = menu
                    .action(
                        tr(cx, "terminal_mode.session_panel.disconnect", "Disconnect"),
                        DisconnectSession {
                            session_id: session_id.0,
                        }
                        .boxed_clone(),
                    )
                    .action(
                        tr(cx, "terminal_mode.session_panel.reconnect", "Reconnect"),
                        ReconnectSession {
                            session_id: session_id.0,
                            background: false,
                        }
                        .boxed_clone(),
                    );
            } else {
                let connect_options = options.clone();
                let connect_handle = panel_handle.clone();
                menu = menu.entry(
                    tr(cx, "terminal_mode.session_panel.connect", "Connect"),
                    None,
                    move |window, cx| {
                        if let Some(panel) = connect_handle.upgrade() {
                            panel.update(cx, |panel, cx| {
                                panel.connect(connect_options.clone(), window, cx);
                            });
                        }
                    },
                );
            }
            menu.separator().entry(
                tr(
                    cx,
                    "terminal_mode.session_panel.manage_servers",
                    "Manage Servers…",
                ),
                None,
                move |window, cx| {
                    if let Some(panel) = panel_handle.upgrade() {
                        panel.update(cx, |panel, cx| panel.open_new_connection(window, cx));
                    }
                },
            )
        });
        window.focus(&context_menu.focus_handle(cx), cx);
        let subscription = cx.subscribe(&context_menu, |panel, _, _: &DismissEvent, cx| {
            panel.context_menu.take();
            cx.notify();
        });
        self.context_menu = Some((context_menu, position, subscription));
        cx.notify();
    }
}

/// Watches the user and global `ssh_config` files, refreshing the panel's host
/// list whenever either changes. The returned tasks must be kept alive.
fn watch_ssh_configs(
    fs: Arc<dyn Fs>,
    panel: WeakEntity<SshSessionPanel>,
    window: &mut Window,
    cx: &mut Context<SshSessionPanel>,
) -> Vec<Task<()>> {
    let mut paths = vec![user_ssh_config_file()];
    if let Some(global) = global_ssh_config_file() {
        paths.push(global.to_path_buf());
    }

    let mut tasks = Vec::new();
    for path in paths {
        let (updates, watcher) = watch_config_file(cx.background_executor(), fs.clone(), path);
        let fs = fs.clone();
        let panel = panel.clone();
        let consumer = cx.spawn_in(window, async move |_panel, cx| {
            let mut updates = updates;
            while updates.next().await.is_some() {
                let hosts = load_ssh_config_hosts(fs.clone()).await;
                panel
                    .update(cx, |panel, cx| {
                        panel.ssh_config_hosts = hosts;
                        cx.notify();
                    })
                    .log_err();
            }
        });
        tasks.push(watcher);
        tasks.push(consumer);
    }
    tasks
}

async fn load_ssh_config_hosts(fs: Arc<dyn Fs>) -> BTreeSet<SharedString> {
    let mut hosts = BTreeSet::new();
    let mut paths = vec![user_ssh_config_file()];
    if let Some(global) = global_ssh_config_file() {
        paths.push(global.to_path_buf());
    }
    for path in paths {
        if let Ok(content) = fs.load(&path).await {
            hosts.extend(
                parse_ssh_config_hosts(&content)
                    .into_iter()
                    .map(SharedString::from),
            );
        }
    }
    hosts
}

fn status_color(status: SessionStatus, cx: &App) -> Hsla {
    let status_colors = cx.theme().status();
    match status {
        SessionStatus::Connecting | SessionStatus::Reconnecting => status_colors.warning,
        SessionStatus::Connected => status_colors.success,
        SessionStatus::Failed => status_colors.error,
    }
}

fn status_label(status: SessionStatus) -> (&'static str, &'static str) {
    match status {
        SessionStatus::Connecting => ("terminal_mode.session.status.connecting", "Connecting…"),
        SessionStatus::Connected => ("terminal_mode.session.status.connected", "Connected"),
        SessionStatus::Reconnecting => {
            ("terminal_mode.session.status.reconnecting", "Reconnecting…")
        }
        SessionStatus::Failed => ("terminal_mode.session.status.failed", "Disconnected"),
    }
}

impl Focusable for SshSessionPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for SshSessionPanel {}

impl Render for SshSessionPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active_session = self.active_session(cx);
        let active_options = active_session.and_then(|id| {
            self.store.upgrade().and_then(|store| {
                store
                    .read(cx)
                    .session(id)
                    .map(|session| session.connection_options.clone())
            })
        });
        let entries = self.entries(cx);
        let is_focused = self.focus_handle.contains_focused(window, cx);

        let broadcast_enabled = self
            .store
            .upgrade()
            .is_some_and(|store| store.read(cx).broadcast_enabled);
        let broadcast_label = if broadcast_enabled {
            tr(cx, "terminal_mode.broadcast.on", "Broadcast: On")
        } else {
            tr(cx, "terminal_mode.broadcast.off", "Broadcast: Off")
        };
        let quick_commands = crate::TerminalModeSettings::get_global(cx)
            .quick_commands
            .clone();

        let filter = v_flex()
            .p_1()
            .gap_1()
            .child(
                Button::new(
                    "terminal-mode-new-connection",
                    tr(
                        cx,
                        "terminal_mode.session_panel.manage_servers",
                        "Manage Servers…",
                    ),
                )
                .on_click(
                    cx.listener(|this, _event, window, cx| this.open_new_connection(window, cx)),
                ),
            )
            .child(self.filter_editor.clone())
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new("terminal-mode-broadcast-toggle", broadcast_label).on_click(
                            cx.listener(|this, _event, _window, cx| {
                                if let Some(store) = this.store.upgrade() {
                                    store.update(cx, |store, cx| {
                                        store.set_broadcast_enabled(!store.broadcast_enabled, cx)
                                    });
                                }
                            }),
                        ),
                    )
                    .child(
                        Button::new(
                            "terminal-mode-broadcast-send-all",
                            tr(cx, "terminal_mode.broadcast.send_all", "Send to All…"),
                        )
                        .on_click(cx.listener(
                            |this, _event, window, cx| this.open_broadcast_modal(window, cx),
                        )),
                    )
                    .when(!quick_commands.is_empty(), |row| {
                        let store = self.store.clone();
                        let commands = quick_commands.clone();
                        row.child(
                            PopoverMenu::new("terminal-mode-quick-commands")
                                .trigger(Button::new(
                                    "terminal-mode-quick-commands-button",
                                    tr(cx, "terminal_mode.quick_commands.action", "Quick Commands"),
                                ))
                                .menu(move |window, cx| {
                                    let store = store.clone();
                                    let commands = commands.clone();
                                    Some(ContextMenu::build(
                                        window,
                                        cx,
                                        move |menu, _window, _cx| {
                                            let mut menu = menu;
                                            for command in &commands {
                                                let store = store.clone();
                                                let command = command.clone();
                                                menu = menu.entry(
                                                    command.clone(),
                                                    None,
                                                    move |_window, cx| {
                                                        if let Some(store) = store.upgrade() {
                                                            store.update(cx, |store, cx| {
                                                                store
                                                                    .run_quick_command(&command, cx)
                                                            });
                                                        }
                                                    },
                                                );
                                            }
                                            menu
                                        },
                                    ))
                                }),
                        )
                    }),
            );

        let mut list = v_flex().flex_1().min_h_0().gap_1().p_1().overflow_hidden();

        if entries.is_empty() {
            let (key, fallback) = if self.filter_query.is_empty() {
                ("terminal_mode.session_panel.empty", "No saved connections")
            } else {
                (
                    "terminal_mode.session_panel.no_matches",
                    "No matching connections",
                )
            };
            list = list.child(Label::new(tr(cx, key, fallback)));
        }

        for (index, entry) in entries.into_iter().enumerate() {
            let is_active = active_options.as_ref() == Some(&entry.options);
            let is_selected = self.selected_index == Some(index);
            let status = self.status_for_options(&entry.options, cx);
            let status_element = status.map(|status| {
                let (key, fallback) = status_label(status);
                div()
                    .id(SharedString::from(format!("terminal-mode-status-{index}")))
                    .size(px(8.))
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(status_color(status, cx))
                    .tooltip(Tooltip::text(tr(cx, key, fallback)))
                    .into_any_element()
            });
            let row = div()
                .id(SharedString::from(format!("terminal-mode-entry-{index}")))
                .cursor_pointer()
                .border_1()
                .border_r_2()
                .rounded_none()
                .hover(|style| {
                    if is_selected {
                        style
                    } else {
                        let hover_color = cx.theme().colors().ghost_element_hover;
                        style.bg(hover_color).border_color(hover_color)
                    }
                })
                .when(is_selected && is_focused, |row| {
                    row.border_color(cx.theme().colors().panel_focused_border)
                })
                .child({
                    let item = ListItem::new(SharedString::from(format!(
                        "terminal-mode-entry-item-{index}"
                    )))
                    .toggle_state(is_selected || is_active)
                    .start_slot(Icon::new(IconName::Server).size(IconSize::Small));
                    let item = match status_element {
                        Some(status_element) => item.end_slot(status_element),
                        None => item,
                    };
                    item.child(
                        v_flex()
                            .flex_1()
                            .overflow_hidden()
                            .child(Label::new(entry.name))
                            .child(
                                Label::new(entry.detail)
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            ),
                    )
                    .on_secondary_mouse_down(cx.listener(
                        move |panel, event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            panel.deploy_context_menu(event.position, index, window, cx);
                        },
                    ))
                })
                .on_click(cx.listener(move |panel, event: &ClickEvent, window, cx| {
                    if event.is_right_click() {
                        return;
                    }
                    panel.activate_entry(index, window, cx);
                }));

            list = list.child(row);
        }

        v_flex()
            .size_full()
            .overflow_hidden()
            .track_focus(&self.focus_handle)
            .key_context(self.dispatch_context(window, cx))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::confirm))
            .child(filter)
            .child(list)
            .children(self.context_menu.as_ref().map(|(menu, position, _)| {
                deferred(
                    anchored()
                        .position(*position)
                        .anchor(Anchor::TopLeft)
                        .child(menu.clone()),
                )
                .with_priority(1)
            }))
    }
}

impl Panel for SshSessionPanel {
    fn persistent_name() -> &'static str {
        "SSH Sessions"
    }

    fn panel_key() -> &'static str {
        SSH_SESSION_PANEL_KEY
    }

    fn position(&self, _window: &Window, _cx: &App) -> DockPosition {
        DockPosition::Left
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        position == DockPosition::Left
    }

    fn set_position(
        &mut self,
        _position: DockPosition,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    fn default_size(&self, _window: &Window, cx: &App) -> Pixels {
        crate::TerminalModeSettings::get_global(cx).session_list_default_width
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<IconName> {
        Some(IconName::Server)
    }

    fn icon_tooltip(&self, _window: &Window, cx: &App) -> Option<SharedString> {
        Some(tr(cx, "terminal_mode.session_panel.title", "SSH Sessions").into())
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleSessionPanel)
    }

    fn activation_priority(&self) -> u32 {
        8
    }
}
