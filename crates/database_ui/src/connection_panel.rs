//! The connection list / schema `Panel`.

use std::sync::Arc;

use database_core::{ConnectionManager, DataSource};
use gpui::*;
use i18n::tr;
use ui::prelude::*;
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

use crate::DatabaseState;

pub struct ConnectionPanel {
    focus_handle: FocusHandle,
    manager: Arc<ConnectionManager>,
}

impl ConnectionPanel {
    pub fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            manager: DatabaseState::manager(cx),
        }
    }

    /// Open a database file chosen through the platform file picker. The path is
    /// resolved to a `ConnectionConfig` and handed to `ResultSetView`.
    pub fn open_database_file(
        _workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let manager = DatabaseState::manager(cx);
        let credentials = DatabaseState::credentials(cx);
        let options = gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        };
        let paths = cx.prompt_for_paths(options);
        cx.spawn_in(window, async move |workspace, cx| {
            let Ok(result) = paths.await else {
                return;
            };
            let selected = match result {
                Ok(Some(paths)) => paths,
                Ok(None) => return,
                Err(_) => return,
            };
            let Some(path) = selected.first() else {
                return;
            };
            let config = config_for_path(path);
            if let Err(error) = workspace.update_in(cx, |workspace, window, cx| {
                let view = cx.new(|cx| {
                    crate::result_set_view::ResultSetView::new(
                        manager.clone(),
                        credentials.clone(),
                        config,
                        window,
                        cx,
                    )
                });
                workspace.add_item_to_active_pane(Box::new(view), None, true, window, cx);
            }) {
                log::error!("failed to open database view: {error:#}");
            }
        })
        .detach();
    }
}

impl Focusable for ConnectionPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for ConnectionPanel {}

impl Render for ConnectionPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let configs = self.manager.configs();
        let mut column = v_flex().size_full().p_2().gap_1();
        column = column.child(div().font_weight(FontWeight::BOLD).child(tr(
            cx,
            "database.panel.title",
            "Databases",
        )));
        if configs.is_empty() {
            column = column.child(div().text_color(cx.theme().colors().text_muted).child(tr(
                cx,
                "database.panel.empty",
                "No connections. Open a .db file to start.",
            )));
        }
        for config in configs {
            column = column.child(
                div().p_1().child(config.display_name.clone()).child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().colors().text_muted)
                        .child(format!("{:?}", config.source)),
                ),
            );
        }
        column
    }
}

impl Panel for ConnectionPanel {
    fn persistent_name() -> &'static str {
        "DatabasePanel"
    }

    fn panel_key() -> &'static str {
        "DatabasePanel"
    }

    fn position(&self, _window: &Window, _cx: &App) -> DockPosition {
        DockPosition::Left
    }

    fn position_is_valid(&self, _position: DockPosition) -> bool {
        true
    }

    fn set_position(
        &mut self,
        _position: DockPosition,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    fn default_size(&self, _window: &Window, _cx: &App) -> Pixels {
        px(280.)
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<ui::IconName> {
        Some(ui::IconName::DatabaseZap)
    }

    fn icon_tooltip(&self, _window: &Window, cx: &App) -> Option<SharedString> {
        Some(tr(cx, "database.panel.title", "Databases").into())
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(crate::ToggleDatabasePanel)
    }

    fn activation_priority(&self) -> u32 {
        10
    }
}

/// Build the non-sensitive connection config for a SQLite file. The id is
/// derived from the path so reopening the same file targets the same connection.
pub(crate) fn config_for_path(path: &std::path::Path) -> database_core::ConnectionConfig {
    database_core::ConnectionConfig {
        id: database_core::ConnectionId(format!("file:{}", path.display())),
        display_name: path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        ),
        driver_id: database_core::SQLITE_DRIVER_ID.to_owned(),
        source: DataSource::File {
            path: path.to_path_buf(),
        },
        host: None,
        port: None,
        user: None,
        database: None,
        options: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::config_for_path;
    use database_core::DataSource;

    #[test]
    fn config_for_path_uses_filename_and_stable_id() {
        let config = config_for_path(std::path::Path::new("/tmp/sample.db"));
        assert_eq!(config.display_name, "sample.db");
        assert_eq!(config.driver_id, database_core::SQLITE_DRIVER_ID);
        assert_eq!(
            config.id,
            database_core::ConnectionId("file:/tmp/sample.db".into())
        );
        assert!(matches!(config.source, DataSource::File { .. }));
        assert!(config.credential_key().starts_with("zzz-database:"));
    }
}
