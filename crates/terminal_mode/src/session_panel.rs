use std::collections::BTreeSet;
use std::sync::Arc;

use ::settings::Settings as _;
use fs::Fs;
use gpui::{
    Action, App, AppContext as _, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle,
    Focusable, InteractiveElement, IntoElement, ParentElement, Pixels, Render, SharedString,
    StatefulInteractiveElement, Styled, WeakEntity, Window,
};
use i18n::tr;
use paths::{global_ssh_config_file, user_ssh_config_file};
use recent_projects::RemoteSettings;
use recent_projects::ssh_config::parse_ssh_config_hosts;
use remote::RemoteConnectionOptions;
use ui::{
    Color, ContextMenu, Icon, IconName, IconSize, Label, LabelSize, prelude::*, right_click_menu,
};
use util::ResultExt as _;
use workspace::{
    Toast, Workspace,
    dock::{DockPosition, Panel, PanelEvent},
    notifications::NotificationId,
};

use crate::session::{SessionId, SessionStore};
use crate::{DisconnectSession, ReconnectSession, ToggleSessionPanel};

const SSH_SESSION_PANEL_KEY: &str = "SshSessionPanel";

struct ConnectionEntry {
    name: SharedString,
    detail: SharedString,
    options: RemoteConnectionOptions,
}

/// Left panel listing saved SSH connections and `~/.ssh/config` hosts (R1).
pub struct SshSessionPanel {
    store: WeakEntity<SessionStore>,
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    ssh_config_hosts: BTreeSet<SharedString>,
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
        let panel = cx.new(|cx| Self {
            store: store.downgrade(),
            workspace: workspace_handle,
            focus_handle: cx.focus_handle(),
            ssh_config_hosts: BTreeSet::new(),
        });

        let panel_handle = panel.downgrade();
        cx.spawn_in(window, async move |_, cx| {
            let hosts = load_ssh_config_hosts(fs).await;
            panel_handle
                .update(cx, |panel, cx| {
                    panel.ssh_config_hosts = hosts;
                    cx.notify();
                })
                .ok();
        })
        .detach();

        panel.update(cx, |_panel, cx| {
            cx.subscribe_in(&store, window, |_panel, _store, _event, _window, cx| {
                cx.notify();
            })
            .detach();
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

        entries
    }

    fn active_session(&self, cx: &App) -> Option<SessionId> {
        self.store
            .upgrade()
            .and_then(|store| store.read(cx).active_session)
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
            if let Err(error) = task.await {
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

impl Focusable for SshSessionPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for SshSessionPanel {}

impl Render for SshSessionPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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

        let mut list = v_flex().size_full().gap_1().p_1().overflow_hidden();

        if entries.is_empty() {
            list = list.child(Label::new(tr(
                cx,
                "terminal_mode.session_panel.empty",
                "No saved SSH connections",
            )));
            return list;
        }

        for (index, entry) in entries.into_iter().enumerate() {
            let is_active = active_options.as_ref() == Some(&entry.options);
            let entry_session_id = self.store.upgrade().and_then(|store| {
                store
                    .read(cx)
                    .sessions
                    .iter()
                    .find(|session| session.connection_options == entry.options)
                    .map(|session| session.id)
            });
            let options = entry.options.clone();
            let session_id = entry_session_id;
            let row = h_flex()
                .id(SharedString::from(format!("terminal-mode-entry-{index}")))
                .w_full()
                .px_2()
                .py_1()
                .gap_2()
                .rounded_md()
                .cursor_pointer()
                .when(is_active, |row| {
                    row.bg(cx.theme().colors().element_selected)
                })
                .hover(|style| style.bg(cx.theme().colors().element_hover))
                .child(Icon::new(IconName::Server).size(IconSize::Small))
                .child(
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
                .on_click(cx.listener(move |panel, _event, window, cx| {
                    if let Some(session_id) = session_id
                        && let Some(store) = panel.store.upgrade()
                    {
                        store.update(cx, |store, cx| store.set_active_session(session_id, cx));
                    } else {
                        panel.connect(options.clone(), window, cx);
                    }
                }));

            let row_element = match entry_session_id {
                Some(session_id) => right_click_menu(SharedString::from(format!(
                    "terminal-mode-entry-menu-{index}"
                )))
                .trigger(move |_is_active, _window, _cx| row)
                .menu(move |window, cx| {
                    ContextMenu::build(window, cx, move |menu, _window, cx| {
                        menu.action(
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
                            }
                            .boxed_clone(),
                        )
                    })
                })
                .into_any_element(),
                None => row.into_any_element(),
            };
            list = list.child(row_element);
        }

        list
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
