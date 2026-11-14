//! A small container panel backed by the `docker` CLI.
//!
//! This is a deliberately lean adaptation of asylum's `docker` crate. The
//! upstream crate speaks the Docker socket over HTTP with its own client; ZZZ
//! only needs the local CLI, which keeps the surface small and needs no
//! network client. Discovery follows `docker ps`, not the daemon API.
//!
//! Adapted from asylum (https://github.com/viniengelage/asylum) at
//! eaab3fb8226637dbd65f8184c1890c5b6556b902, licensed GPL-3.0-or-later.

use gpui::{
    Action as _, AnyElement, App, Context, EventEmitter, FocusHandle, Focusable, Pixels, Render,
    SharedString, Window, actions, px,
};
use i18n::tr;
use serde_json::Value;
use ui::{
    Button, ButtonSize, ButtonStyle, Color, Icon, IconName, IconSize, Label, LabelSize, prelude::*,
};
use util::{ResultExt as _, command::new_command};
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

actions!(
    docker,
    [
        /// Opens or focuses the container panel.
        ToggleDockerPanel,
        /// Re-reads the container list from the Docker CLI.
        RefreshDockerContainers,
    ]
);

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _window, _cx| {
        workspace.register_action(|workspace, _: &ToggleDockerPanel, window, cx| {
            if !workspace.toggle_panel_focus::<DockerPanel>(window, cx) {
                open(workspace, window, cx);
            }
        });
        workspace.register_action(|workspace, _: &RefreshDockerContainers, window, cx| {
            match workspace.panel::<DockerPanel>(cx) {
                Some(panel) => {
                    panel.update(cx, |panel, cx| panel.refresh(cx));
                }
                None => open(workspace, window, cx),
            }
        });
    })
    .detach();
}

fn open(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    if workspace.panel::<DockerPanel>(cx).is_none() {
        let panel = cx.new(|cx| DockerPanel::new(window, cx));
        workspace.add_panel(panel, window, cx);
    }
    workspace.focus_panel::<DockerPanel>(window, cx);
}

#[derive(Clone, Debug)]
struct Container {
    id: String,
    name: String,
    image: String,
    status: String,
    running: bool,
}

impl Container {
    fn from_json(line: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(line).ok()?;
        let field = |name: &str| value.get(name).and_then(Value::as_str).unwrap_or_default();
        Some(Self {
            id: field("ID").to_owned(),
            name: field("Names").to_owned(),
            image: field("Image").to_owned(),
            status: field("Status").to_owned(),
            running: field("State") == "running",
        })
    }
}

#[derive(Clone, Copy)]
enum ContainerCommand {
    Start,
    Stop,
}

impl ContainerCommand {
    fn argument(self) -> &'static str {
        match self {
            ContainerCommand::Start => "start",
            ContainerCommand::Stop => "stop",
        }
    }
}

pub struct DockerPanel {
    focus_handle: FocusHandle,
    containers: Vec<Container>,
    error: Option<SharedString>,
    loading: bool,
    loaded: bool,
}

impl DockerPanel {
    fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let panel = Self {
            focus_handle: cx.focus_handle(),
            containers: Vec::new(),
            error: None,
            loading: false,
            loaded: false,
        };
        panel
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        self.loaded = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let mut command = new_command("docker");
            command.args(["ps", "-a", "--format", "{{json .}}"]);
            let result = command.output().await;
            this.update(cx, |this, cx| {
                this.loading = false;
                match result {
                    Ok(output) if output.status.success() => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        this.containers = stdout.lines().filter_map(Container::from_json).collect();
                        this.error = None;
                    }
                    Ok(output) => {
                        this.error = Some(
                            String::from_utf8_lossy(&output.stderr)
                                .trim()
                                .to_owned()
                                .into(),
                        );
                    }
                    Err(error) => {
                        this.error = Some(error.to_string().into());
                    }
                }
                cx.notify();
            })
            .log_err();
        })
        .detach();
    }

    fn run_command(&mut self, id: String, command: ContainerCommand, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let mut docker = new_command("docker");
            docker.args([command.argument(), id.as_str()]);
            let result = docker.output().await;
            this.update(cx, |this, cx| {
                if let Err(error) = result {
                    this.error = Some(error.to_string().into());
                }
                this.refresh(cx);
            })
            .log_err();
        })
        .detach();
    }

    fn render_row(&self, container: &Container, cx: &mut Context<Self>) -> impl IntoElement {
        let id = container.id.clone();
        let start_id = id.clone();
        let running = container.running;
        let (action_label, button_style) = if running {
            (tr(cx, "docker.stop", "Stop"), ButtonStyle::Subtle)
        } else {
            (
                tr(cx, "docker.start", "Start"),
                ButtonStyle::Tinted(ui::TintColor::Accent),
            )
        };

        h_flex()
            .id(SharedString::from(format!(
                "docker-container-{}",
                container.id
            )))
            .justify_between()
            .gap_2()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(cx.theme().colors().border_variant)
            .child(
                v_flex()
                    .min_w_0()
                    .child(
                        Label::new(container.name.clone())
                            .size(LabelSize::Small)
                            .truncate(),
                    )
                    .child(
                        Label::new(format!("{} · {}", container.image, container.status))
                            .size(LabelSize::XSmall)
                            .color(Color::Muted)
                            .truncate(),
                    ),
            )
            .child(
                Button::new(
                    SharedString::from(format!("docker-command-{}", container.id)),
                    action_label,
                )
                .style(button_style)
                .size(ButtonSize::Compact)
                .label_size(LabelSize::XSmall)
                .on_click(cx.listener(move |this, _, _, cx| {
                    let command = if running {
                        ContainerCommand::Stop
                    } else {
                        ContainerCommand::Start
                    };
                    this.run_command(start_id.clone(), command, cx);
                })),
            )
    }
}

impl Render for DockerPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.loaded {
            self.refresh(cx);
        }

        let content = if let Some(error) = &self.error {
            v_flex()
                .p_2()
                .gap_1()
                .child(
                    h_flex()
                        .gap_1()
                        .child(
                            Icon::new(IconName::XCircle)
                                .size(IconSize::XSmall)
                                .color(Color::Error),
                        )
                        .child(
                            Label::new(tr(cx, "docker.unavailable", "Docker is unavailable"))
                                .size(LabelSize::Small)
                                .color(Color::Error),
                        ),
                )
                .child(
                    Label::new(error.clone())
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
                .into_any_element()
        } else if self.containers.is_empty() {
            v_flex()
                .p_2()
                .child(
                    Label::new(tr(cx, "docker.no_containers", "No containers."))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .into_any_element()
        } else {
            let rows: Vec<AnyElement> = self
                .containers
                .iter()
                .map(|container| self.render_row(container, cx).into_any_element())
                .collect();
            v_flex().children(rows).into_any_element()
        };

        v_flex()
            .key_context("DockerPanel")
            .track_focus(&self.focus_handle)
            .size_full()
            .child(
                h_flex()
                    .justify_between()
                    .px_2()
                    .py_1()
                    .border_b_1()
                    .border_color(cx.theme().colors().border_variant)
                    .child(
                        Label::new(tr(cx, "docker.title", "Containers"))
                            .size(LabelSize::Small)
                            .weight(gpui::FontWeight::SEMIBOLD),
                    )
                    .child(
                        Button::new("docker-refresh", tr(cx, "docker.refresh", "Refresh"))
                            .style(ButtonStyle::Subtle)
                            .size(ButtonSize::Compact)
                            .label_size(LabelSize::XSmall)
                            .loading(self.loading)
                            .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                    ),
            )
            .child(
                div()
                    .id("docker-container-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(content),
            )
    }
}

impl Focusable for DockerPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for DockerPanel {}

impl Panel for DockerPanel {
    fn persistent_name() -> &'static str {
        "DockerPanel"
    }

    fn panel_key() -> &'static str {
        "DockerPanel"
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

    fn default_size(&self, _window: &Window, _cx: &App) -> Pixels {
        px(360.)
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<IconName> {
        None
    }

    fn icon_tooltip(&self, _window: &Window, cx: &App) -> Option<SharedString> {
        Some(tr(cx, "docker.title", "Containers").into())
    }

    fn toggle_action(&self) -> Box<dyn gpui::Action> {
        ToggleDockerPanel.boxed_clone()
    }

    fn activation_priority(&self) -> u32 {
        9
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_container_line() {
        let container = Container::from_json(
            r#"{"ID":"abc","Names":"web","Image":"nginx:latest","State":"running","Status":"Up 2 hours"}"#,
        )
        .expect("line should parse");
        assert_eq!(container.name, "web");
        assert!(container.running);
    }

    #[test]
    fn ignores_non_json_lines() {
        assert!(Container::from_json("docker: command not found").is_none());
    }
}
