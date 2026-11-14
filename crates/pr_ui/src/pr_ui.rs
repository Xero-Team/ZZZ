//! A pull-request panel backed by the `gh` CLI.
//!
//! This is a deliberately lean adaptation of lathe's `pr_ui` crate. Upstream
//! stores PR metadata and cached diffs in the local DB, authenticates through
//! the git credential helper and renders review threads in the editor. ZZZ only
//! needs a read-only review surface, so this panel asks the locally installed
//! `gh` for the data and renders the diff itself. No hosted endpoint, token
//! store or sign-in flow is involved: `gh` owns its own credentials.
//!
//! Adapted from lathe (https://github.com/paterschris/lathe) at
//! 44d467a53adc884a41e8015f922f00e628841690, licensed GPL-3.0-or-later.

use gpui::{
    Action as _, AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, Pixels,
    Render, SharedString, Window, actions, px,
};
use i18n::tr;
use project::Project;
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
    pr_ui,
    [
        /// Opens or focuses the pull-request panel.
        TogglePullRequestsPanel,
        /// Re-reads the pull-request list from the `gh` CLI.
        RefreshPullRequests,
    ]
);

/// Diffs above this many lines are truncated; a side panel is not a pager, and
/// rendering tens of thousands of labels stalls the frame.
const MAX_DIFF_LINES: usize = 4000;

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _window, _cx| {
        workspace.register_action(|workspace, _: &TogglePullRequestsPanel, window, cx| {
            if !workspace.toggle_panel_focus::<PullRequestPanel>(window, cx) {
                open(workspace, window, cx);
            }
        });
        workspace.register_action(|workspace, _: &RefreshPullRequests, window, cx| {
            match workspace.panel::<PullRequestPanel>(cx) {
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
    if workspace.panel::<PullRequestPanel>(cx).is_none() {
        let project = workspace.project().clone();
        let panel = cx.new(|cx| PullRequestPanel::new(project, window, cx));
        workspace.add_panel(panel, window, cx);
    }
    workspace.focus_panel::<PullRequestPanel>(window, cx);
}

/// The directory `gh` runs in, so it can resolve the repository from the
/// project's worktree instead of the editor's own working directory.
fn worktree_root(project: &Project, cx: &App) -> Option<std::path::PathBuf> {
    project
        .worktrees(cx)
        .next()
        .map(|worktree| worktree.read(cx).abs_path().to_path_buf())
}

fn gh_command(root: &std::path::Path) -> util::command::Command {
    let mut command = new_command("gh");
    command.current_dir(root);
    command
}

#[derive(Clone, Debug, PartialEq)]
struct PullRequest {
    number: u64,
    title: String,
    author: String,
    head_ref: String,
    draft: bool,
    review_decision: String,
    url: String,
}

impl PullRequest {
    fn from_json(value: &Value) -> Option<Self> {
        let number = value.get("number")?.as_u64()?;
        let field = |name: &str| {
            value
                .get(name)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let author = value
            .get("author")
            .and_then(|author| author.get("login"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        Some(Self {
            number,
            title: field("title"),
            author,
            head_ref: field("headRefName"),
            draft: value
                .get("isDraft")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            review_decision: field("reviewDecision"),
            url: field("url"),
        })
    }

    fn parse_list(stdout: &str) -> Vec<Self> {
        match serde_json::from_str::<Vec<Value>>(stdout) {
            Ok(values) => values.iter().filter_map(Self::from_json).collect(),
            // `gh` prints its own diagnostics (not JSON) when it is not
            // authenticated or cannot resolve the repository.
            Err(error) => {
                log::debug!("could not parse `gh pr list` output: {error}");
                Vec::new()
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiffLineKind {
    Header,
    Addition,
    Removal,
    Context,
}

impl DiffLineKind {
    fn classify(line: &str) -> Self {
        if line.starts_with("+++")
            || line.starts_with("---")
            || line.starts_with("diff --git")
            || line.starts_with("index ")
        {
            Self::Header
        } else if line.starts_with("@@") {
            Self::Header
        } else if line.starts_with('+') {
            Self::Addition
        } else if line.starts_with('-') {
            Self::Removal
        } else {
            Self::Context
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct DiffLine {
    kind: DiffLineKind,
    text: String,
}

fn parse_diff(stdout: &str) -> Vec<DiffLine> {
    stdout
        .lines()
        .take(MAX_DIFF_LINES)
        .map(|line| DiffLine {
            kind: DiffLineKind::classify(line),
            text: line.to_owned(),
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Loading {
    Idle,
    List,
    Diff,
}

pub struct PullRequestPanel {
    focus_handle: FocusHandle,
    project: Entity<Project>,
    pull_requests: Vec<PullRequest>,
    diff: Option<Vec<DiffLine>>,
    selected: Option<PullRequest>,
    error: Option<SharedString>,
    status: Loading,
    root: Option<std::path::PathBuf>,
    loaded: bool,
}

impl PullRequestPanel {
    fn new(project: Entity<Project>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            project,
            pull_requests: Vec::new(),
            diff: None,
            selected: None,
            error: None,
            status: Loading::Idle,
            root: None,
            loaded: false,
        }
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let root = worktree_root(self.project.read(cx), cx);
        self.status = Loading::List;
        self.error = None;
        self.selected = None;
        self.diff = None;
        self.loaded = true;
        cx.notify();

        let Some(root) = root else {
            self.fail(
                tr(
                    cx,
                    "pr_ui.no_project",
                    "Open a project to list pull requests.",
                )
                .into(),
                cx,
            );
            return;
        };
        self.root = Some(root.clone());

        cx.spawn(async move |this, cx| {
            let mut command = gh_command(&root);
            command.args([
                "pr",
                "list",
                "--limit",
                "50",
                "--json",
                "number,title,author,headRefName,isDraft,reviewDecision,url",
            ]);
            let result = command.output().await;

            this.update(cx, |this, cx| {
                this.status = Loading::Idle;
                match result {
                    Ok(output) if output.status.success() => {
                        this.pull_requests =
                            PullRequest::parse_list(&String::from_utf8_lossy(&output.stdout));
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

    fn select(&mut self, pull_request: PullRequest, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };

        self.status = Loading::Diff;
        self.selected = Some(pull_request.clone());
        self.diff = None;
        self.error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let mut command = gh_command(&root);
            command.args(["pr", "diff", &pull_request.number.to_string()]);
            let result = command.output().await;

            this.update(cx, |this, cx| {
                this.status = Loading::Idle;
                match result {
                    Ok(output) if output.status.success() => {
                        this.diff = Some(parse_diff(&String::from_utf8_lossy(&output.stdout)));
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

    fn fail(&mut self, message: SharedString, cx: &mut Context<Self>) {
        self.status = Loading::Idle;
        self.error = Some(message);
        cx.notify();
    }

    fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        self.diff = None;
        self.error = None;
        cx.notify();
    }

    fn render_row(&self, pull_request: &PullRequest, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = pull_request.clone();
        let mut badges = String::new();
        if pull_request.draft {
            badges.push_str(&tr(cx, "pr_ui.draft", "Draft"));
        }
        if !pull_request.review_decision.is_empty() {
            if !badges.is_empty() {
                badges.push_str(" · ");
            }
            badges.push_str(&pull_request.review_decision.to_lowercase());
        }

        v_flex()
            .id(SharedString::from(format!(
                "pr-row-{}",
                pull_request.number
            )))
            .gap_0p5()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(cx.theme().colors().border_variant)
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().colors().element_hover))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.select(selected.clone(), cx);
            }))
            .child(
                Label::new(pull_request.title.clone())
                    .size(LabelSize::Small)
                    .truncate(),
            )
            .child(
                Label::new(format!(
                    "#{} · {} · {}",
                    pull_request.number, pull_request.author, pull_request.head_ref
                ))
                .size(LabelSize::XSmall)
                .color(Color::Muted)
                .truncate(),
            )
            .when(!badges.is_empty(), |el| {
                el.child(
                    Label::new(badges)
                        .size(LabelSize::XSmall)
                        .color(Color::Muted)
                        .truncate(),
                )
            })
    }

    fn render_diff(&self, pull_request: &PullRequest, cx: &mut Context<Self>) -> impl IntoElement {
        let url = pull_request.url.clone();
        let header = h_flex()
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
                        Label::new(format!("#{} {}", pull_request.number, pull_request.title))
                            .size(LabelSize::Small)
                            .truncate(),
                    )
                    .child(
                        Label::new(format!(
                            "#{} · {}",
                            pull_request.number, pull_request.author
                        ))
                        .size(LabelSize::XSmall)
                        .color(Color::Muted)
                        .truncate(),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new("pr-open-in-browser", tr(cx, "pr_ui.open", "Open"))
                            .style(ButtonStyle::Subtle)
                            .size(ButtonSize::Compact)
                            .label_size(LabelSize::XSmall)
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.open_url(&url);
                            })),
                    )
                    .child(
                        Button::new("pr-back", tr(cx, "pr_ui.back", "Back"))
                            .style(ButtonStyle::Subtle)
                            .size(ButtonSize::Compact)
                            .label_size(LabelSize::XSmall)
                            .on_click(cx.listener(|this, _, _, cx| this.clear_selection(cx))),
                    ),
            );

        let body: AnyElement = if self.status == Loading::Diff {
            v_flex()
                .p_2()
                .child(
                    Label::new(tr(cx, "pr_ui.loading_diff", "Loading diff…"))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .into_any_element()
        } else if let Some(diff) = &self.diff {
            if diff.is_empty() {
                v_flex()
                    .p_2()
                    .child(
                        Label::new(tr(cx, "pr_ui.no_diff", "This pull request has no diff."))
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .into_any_element()
            } else {
                v_flex()
                    .children(diff.iter().map(|line| {
                        let color = match line.kind {
                            DiffLineKind::Addition => Color::Success,
                            DiffLineKind::Removal => Color::Error,
                            DiffLineKind::Header => Color::Accent,
                            DiffLineKind::Context => Color::Default,
                        };
                        h_flex().px_2().child(
                            Label::new(line.text.clone())
                                .size(LabelSize::XSmall)
                                .color(color),
                        )
                    }))
                    .into_any_element()
            }
        } else {
            v_flex()
                .p_2()
                .child(
                    Label::new(tr(cx, "pr_ui.no_diff", "This pull request has no diff."))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .into_any_element()
        };

        v_flex().child(header).child(
            div()
                .id("pr-diff")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(body),
        )
    }
}

impl Render for PullRequestPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.loaded {
            self.refresh(cx);
        }

        let body: AnyElement = match self.selected.clone() {
            Some(selected) => self.render_diff(&selected, cx).into_any_element(),
            None => self.render_list(cx).into_any_element(),
        };

        v_flex()
            .key_context("PullRequestPanel")
            .track_focus(&self.focus_handle)
            .size_full()
            .child(body)
    }
}

impl PullRequestPanel {
    fn render_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let content: AnyElement = if let Some(error) = &self.error {
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
                            Label::new(tr(cx, "pr_ui.unavailable", "GitHub CLI is unavailable"))
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
        } else if self.status == Loading::List {
            v_flex()
                .p_2()
                .child(
                    Label::new(tr(cx, "pr_ui.loading", "Loading pull requests…"))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .into_any_element()
        } else if self.pull_requests.is_empty() {
            v_flex()
                .p_2()
                .child(
                    Label::new(tr(cx, "pr_ui.none", "No open pull requests."))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .into_any_element()
        } else {
            let rows: Vec<AnyElement> = self
                .pull_requests
                .iter()
                .map(|pull_request| self.render_row(pull_request, cx).into_any_element())
                .collect();
            v_flex().children(rows).into_any_element()
        };

        v_flex()
            .key_context("PullRequestPanel")
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
                        Label::new(tr(cx, "pr_ui.title", "Pull requests"))
                            .size(LabelSize::Small)
                            .weight(gpui::FontWeight::SEMIBOLD),
                    )
                    .child(
                        Button::new("pr-refresh", tr(cx, "pr_ui.refresh", "Refresh"))
                            .style(ButtonStyle::Subtle)
                            .size(ButtonSize::Compact)
                            .label_size(LabelSize::XSmall)
                            .loading(self.status == Loading::List)
                            .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                    ),
            )
            .child(
                div()
                    .id("pr-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(content),
            )
    }
}

impl Focusable for PullRequestPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for PullRequestPanel {}

impl Panel for PullRequestPanel {
    fn persistent_name() -> &'static str {
        "PullRequestPanel"
    }

    fn panel_key() -> &'static str {
        "PullRequestPanel"
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
        px(420.)
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<IconName> {
        Some(IconName::GitBranch)
    }

    fn icon_tooltip(&self, _window: &Window, cx: &App) -> Option<SharedString> {
        Some(tr(cx, "pr_ui.title", "Pull requests").into())
    }

    fn toggle_action(&self) -> Box<dyn gpui::Action> {
        TogglePullRequestsPanel.boxed_clone()
    }

    fn activation_priority(&self) -> u32 {
        8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pull_request_list() {
        let pull_requests = PullRequest::parse_list(
            r#"[{"number":7,"title":"Fix","author":{"login":"begonia"},"headRefName":"fix","isDraft":false,"reviewDecision":"APPROVED","url":"https://example.invalid/pr/7"}]"#,
        );
        assert_eq!(pull_requests.len(), 1);
        assert_eq!(pull_requests[0].number, 7);
        assert_eq!(pull_requests[0].author, "begonia");
        assert_eq!(pull_requests[0].review_decision, "APPROVED");
    }

    #[test]
    fn ignores_malformed_pull_request_lists() {
        assert!(PullRequest::parse_list("gh: not authenticated").is_empty());
    }

    #[test]
    fn classifies_diff_lines() {
        assert_eq!(DiffLineKind::classify("+++ b/x"), DiffLineKind::Header);
        assert_eq!(DiffLineKind::classify("@@ -1 +1 @@"), DiffLineKind::Header);
        assert_eq!(DiffLineKind::classify("+added"), DiffLineKind::Addition);
        assert_eq!(DiffLineKind::classify("-removed"), DiffLineKind::Removal);
        assert_eq!(DiffLineKind::classify(" context"), DiffLineKind::Context);
    }
}
