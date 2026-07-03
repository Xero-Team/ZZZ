//! A UI interface for managing the [`TrustedWorktrees`] data.

use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    sync::Arc,
};

use collections::{HashMap, HashSet};
use gpui::{DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, ScrollHandle, WeakEntity};
use i18n::tr;

use project::{
    WorktreeId,
    trusted_worktrees::{PathTrust, RemoteHostLocation, TrustedWorktrees},
    worktree_store::WorktreeStore,
};
use smallvec::SmallVec;
use theme::ActiveTheme;
use ui::{
    AlertModal, Checkbox, FluentBuilder, KeyBinding, ListBulletItem, ToggleState, WithScrollbar,
    prelude::*,
};
use ui_input::InputField;

use util::paths::PathStyle;

use crate::{DismissDecision, ModalView, ToggleWorktreeSecurity};

pub struct SecurityModal {
    restricted_paths: HashMap<WorktreeId, RestrictedPath>,
    home_dir: Option<PathBuf>,
    trust_parents: bool,
    worktree_store: WeakEntity<WorktreeStore>,
    remote_host: Option<RemoteHostLocation>,
    focus_handle: FocusHandle,
    project_list_scroll_handle: ScrollHandle,
    trusted: Option<bool>,
    trust_path_input: Entity<InputField>,
    trust_path_error: Option<SharedString>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrustScopeValidationError {
    Empty,
    NotAbsolute,
    NotAncestor,
}

impl TrustScopeValidationError {
    fn message(self, cx: &App) -> SharedString {
        match self {
            Self::Empty => tr(
                cx,
                "workspace.security_modal.error.enter_folder_to_trust",
                "Enter a folder to trust",
            )
            .into(),
            Self::NotAbsolute => tr(
                cx,
                "workspace.security_modal.error.enter_absolute_folder_path",
                "Enter an absolute folder path",
            )
            .into(),
            Self::NotAncestor => tr(
                cx,
                "workspace.security_modal.error.must_be_parent_folder",
                "Must be a parent folder of the project",
            )
            .into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct RestrictedPath {
    abs_path: Arc<Path>,
    is_file: bool,
    host: Option<RemoteHostLocation>,
}

impl Focusable for SecurityModal {
    fn focus_handle(&self, _: &ui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<DismissEvent> for SecurityModal {}

impl ModalView for SecurityModal {
    fn fade_out_background(&self) -> bool {
        true
    }

    fn on_before_dismiss(&mut self, _: &mut Window, _: &mut Context<Self>) -> DismissDecision {
        DismissDecision::Dismiss(true)
    }
}

impl Render for SecurityModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.restricted_paths.is_empty() {
            self.dismiss(cx);
            return v_flex().into_any_element();
        }

        let restricted_count = self.restricted_paths.len();
        let header_label: SharedString = if restricted_count == 1 {
            tr(
                cx,
                "workspace.security_modal.unrecognized_project",
                "Unrecognized Project",
            )
            .into()
        } else {
            tr(
                cx,
                "workspace.security_modal.unrecognized_projects",
                "Unrecognized Projects ({})",
            )
            .replacen("{}", &restricted_count.to_string(), 1)
            .into()
        };

        let trust_label = self.build_trust_label(cx);
        let trust_input = self
            .single_trustable_path()
            .is_some()
            .then(|| self.trust_path_input.clone());

        AlertModal::new("security-modal")
            .width(rems(40.))
            .key_context("SecurityModal")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(|this, _: &menu::Confirm, _window, cx| {
                this.trust_and_dismiss(cx);
            }))
            .on_action(cx.listener(|security_modal, _: &ToggleWorktreeSecurity, _window, cx| {
                security_modal.trusted = Some(false);
                security_modal.dismiss(cx);
            }))
            .header(
                v_flex()
                    .p_3()
                    .gap_1()
                    .rounded_t_md()
                    .bg(cx.theme().colors().editor_background.opacity(0.5))
                    .border_b_1()
                    .border_color(cx.theme().colors().border_variant)
                    .child(
                        h_flex()
                            .gap_2()
                            .child(Icon::new(IconName::Warning).color(Color::Warning))
                            .child(Label::new(header_label)),
                    )
                    .child(
                        div()
                            .size_full()
                            .vertical_scrollbar_for(&self.project_list_scroll_handle, window, cx)
                            .child(
                                v_flex()
                                    .id("paths_container")
                                    .max_h_24()
                                    .overflow_y_scroll()
                                    .track_scroll(&self.project_list_scroll_handle)
                                    .children(
                                        self.restricted_paths.values().filter_map(
                                            |restricted_path| {
                                                let abs_path = if restricted_path.is_file {
                                                    restricted_path.abs_path.parent()
                                                } else {
                                                    Some(restricted_path.abs_path.as_ref())
                                                }?;
                                                let label = match &restricted_path.host {
                                                    Some(remote_host) => {
                                                        match &remote_host.user_name {
                                                            Some(user_name) => format!(
                                                                "{} ({}@{})",
                                                                self.shorten_path(abs_path)
                                                                    .display(),
                                                                user_name,
                                                                remote_host.host_identifier
                                                            ),
                                                            None => format!(
                                                                "{} ({})",
                                                                self.shorten_path(abs_path)
                                                                    .display(),
                                                                remote_host.host_identifier
                                                            ),
                                                        }
                                                    }
                                                    None => self
                                                        .shorten_path(abs_path)
                                                        .display()
                                                        .to_string(),
                                                };
                                                Some(
                                                    h_flex()
                                                        .pl(
                                                            IconSize::default().rems() + rems(0.5),
                                                        )
                                                        .child(
                                                            Label::new(label).color(Color::Muted),
                                                        ),
                                                )
                                            },
                                        ),
                                    ),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        v_flex()
                            .child(
                                Label::new(
                                    tr(
                                        cx,
                                        "workspace.security_modal.untrusted_projects_message",
                                        "Untrusted projects are opened in Restricted Mode to protect your system.",
                                    ),
                                )
                                .color(Color::Muted),
                            )
                            .child(
                                Label::new(
                                    tr(
                                        cx,
                                        "workspace.security_modal.review_settings_message",
                                        "Review .ZZZ/settings.json for any extensions or commands configured by this project.",
                                    ),
                                )
                                .color(Color::Muted),
                            ),
                    )
                    .child(
                        v_flex()
                            .child(
                                Label::new(tr(
                                    cx,
                                    "workspace.security_modal.restricted_mode_prevents",
                                    "Restricted Mode prevents:",
                                ))
                                .color(Color::Muted),
                            )
                            .child(ListBulletItem::new(tr(
                                cx,
                                "workspace.security_modal.prevents_project_settings",
                                "Project settings from being applied",
                            )))
                            .child(ListBulletItem::new(tr(
                                cx,
                                "workspace.security_modal.prevents_language_servers",
                                "Language servers from running",
                            )))
                            .child(ListBulletItem::new(tr(
                                cx,
                                "workspace.security_modal.prevents_mcp_integrations",
                                "MCP Server integrations from installing",
                            ))),
                    )
                    .map(|this| {
                        let Some(trust_label) = trust_label else {
                            return this;
                        };

                        match trust_input {
                            Some(input) => this.child(
                                v_flex()
                                    .gap_1()
                                    .child(
                                        h_flex()
                                            .items_start()
                                            .gap_1p5()
                                            .child(
                                                h_flex()
                                                    .h_8()
                                                    .child(
                                                        Checkbox::new(
                                                            "trust-parents",
                                                            ToggleState::from(self.trust_parents),
                                                        )
                                                        .label(tr(
                                                            cx,
                                                            "workspace.security_modal.trust_all_projects_in",
                                                            "Trust all projects in",
                                                        ))
                                                        .on_click(cx.listener(
                                                            |security_modal,
                                                             state: &ToggleState,
                                                             _,
                                                             cx| {
                                                                security_modal.trust_parents =
                                                                    state.selected();
                                                                if !security_modal.trust_parents {
                                                                    security_modal
                                                                        .trust_path_error = None;
                                                                }
                                                                cx.notify();
                                                                cx.stop_propagation();
                                                            },
                                                        )),
                                                    ),
                                            )
                                            .child(input),
                                    )
                                    .when_some(self.trust_path_error.clone(), |this, error| {
                                        this.child(
                                            Label::new(error)
                                                .size(LabelSize::Small)
                                                .color(Color::Error),
                                        )
                                    }),
                            ),
                            None => this.child(
                                Checkbox::new("trust-parents", ToggleState::from(self.trust_parents))
                                    .label(trust_label)
                                    .on_click(cx.listener(
                                        |security_modal, state: &ToggleState, _, cx| {
                                            security_modal.trust_parents = state.selected();
                                            cx.notify();
                                            cx.stop_propagation();
                                        },
                                    )),
                            ),
                        }
                    }),
            )
            .footer(
                h_flex()
                    .px_3()
                    .pb_3()
                    .gap_1()
                    .justify_end()
                    .child(
                        Button::new(
                            "rm",
                            tr(
                                cx,
                                "workspace.security_modal.stay_in_restricted_mode",
                                "Stay in Restricted Mode",
                            ),
                        )
                            .key_binding(
                                KeyBinding::for_action(
                                    &ToggleWorktreeSecurity,
                                    cx,
                                )
                                .map(|kb| kb.size(rems_from_px(12.))),
                            )
                            .on_click(cx.listener(move |security_modal, _, _, cx| {
                                security_modal.trusted = Some(false);
                                security_modal.dismiss(cx);
                                cx.stop_propagation();
                            })),
                    )
                    .child(
                        Button::new(
                            "tc",
                            tr(
                                cx,
                                "workspace.security_modal.trust_and_continue",
                                "Trust and Continue",
                            ),
                        )
                            .style(ButtonStyle::Filled)
                            .layer(ui::ElevationIndex::ModalSurface)
                            .key_binding(
                                KeyBinding::for_action(&menu::Confirm, cx)
                                    .map(|kb| kb.size(rems_from_px(12.))),
                            )
                            .on_click(cx.listener(move |security_modal, _, _, cx| {
                                security_modal.trust_and_dismiss(cx);
                                cx.stop_propagation();
                            })),
                    ),
            )
            .into_any_element()
    }
}

impl SecurityModal {
    pub fn new(
        worktree_store: WeakEntity<WorktreeStore>,
        remote_host: Option<impl Into<RemoteHostLocation>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let trust_path_input = cx.new(|cx| {
            let placeholder = tr(
                cx,
                "workspace.security_modal.folder_to_trust",
                "Folder to trust",
            );
            InputField::new(window, cx, &placeholder)
        });
        let mut this = Self {
            worktree_store,
            remote_host: remote_host.map(|host| host.into()),
            restricted_paths: HashMap::default(),
            focus_handle: cx.focus_handle(),
            project_list_scroll_handle: ScrollHandle::new(),
            trust_parents: false,
            home_dir: std::env::home_dir(),
            trusted: None,
            trust_path_input,
            trust_path_error: None,
        };
        this.refresh_restricted_paths(cx);

        if let Some(project) = this.single_trustable_path() {
            let default_scope = project.parent().unwrap_or(project.as_ref()).to_path_buf();
            this.trust_path_input.update(cx, |field, cx| {
                field.set_text(&default_scope.to_string_lossy(), window, cx);
            });
        }

        this
    }

    fn build_trust_label(&self, cx: &App) -> Option<Cow<'static, str>> {
        let mut has_restricted_files = false;
        let available_parents = self
            .restricted_paths
            .values()
            .filter(|restricted_path| {
                has_restricted_files |= restricted_path.is_file;
                !restricted_path.is_file
            })
            .filter_map(|restricted_path| restricted_path.abs_path.parent())
            .collect::<SmallVec<[_; 2]>>();
        match available_parents.len() {
            0 => {
                if has_restricted_files {
                    Some(Cow::Owned(tr(
                        cx,
                        "workspace.security_modal.trust_all_single_files",
                        "Trust all single files",
                    )))
                } else {
                    None
                }
            }
            1 => Some(Cow::Owned(
                tr(
                    cx,
                    "workspace.security_modal.trust_all_projects_in_folder",
                    "Trust all projects in the {} folder",
                )
                .replacen(
                    "{}",
                    &self
                        .shorten_path(available_parents[0])
                        .display()
                        .to_string(),
                    1,
                ),
            )),
            _ => Some(Cow::Owned(tr(
                cx,
                "workspace.security_modal.trust_all_projects_in_parent_folders",
                "Trust all projects in the parent folders",
            ))),
        }
    }

    fn shorten_path<'a>(&self, path: &'a Path) -> Cow<'a, Path> {
        match &self.home_dir {
            Some(home_dir) => path
                .strip_prefix(home_dir)
                .map(|stripped| Path::new("~").join(stripped))
                .map(Cow::Owned)
                .unwrap_or(Cow::Borrowed(path)),
            None => Cow::Borrowed(path),
        }
    }

    fn edited_trust_scope(&self, cx: &App) -> Result<Option<PathBuf>, SharedString> {
        if !self.trust_parents {
            return Ok(None);
        }

        let Some(project) = self.single_trustable_path() else {
            return Ok(None);
        };

        let typed = self.trust_path_input.read(cx).text(cx);
        let path_style = self
            .worktree_store
            .upgrade()
            .map(|store| store.read(cx).path_style())
            .unwrap_or_else(PathStyle::local);
        validate_trust_scope(&typed, &project, self.home_dir.as_deref(), path_style)
            .map(Some)
            .map_err(|error| error.message(cx))
    }

    fn trust_and_dismiss(&mut self, cx: &mut Context<Self>) {
        let scope_override = match self.edited_trust_scope(cx) {
            Ok(scope_override) => {
                self.trust_path_error = None;
                scope_override
            }
            Err(error) => {
                self.trust_path_error = Some(error);
                cx.notify();
                return;
            }
        };

        if let Some((trusted_worktrees, worktree_store)) =
            TrustedWorktrees::try_get_global(cx).zip(self.worktree_store.upgrade())
        {
            trusted_worktrees.update(cx, |trusted_worktrees, cx| {
                let mut paths_to_trust = self
                    .restricted_paths
                    .keys()
                    .copied()
                    .map(PathTrust::Worktree)
                    .collect::<HashSet<_>>();
                if self.trust_parents {
                    if let Some(scope_override) = scope_override.clone() {
                        paths_to_trust.insert(PathTrust::AbsPath(scope_override));
                    } else {
                        paths_to_trust.extend(self.restricted_paths.values().filter_map(
                            |restricted_paths| {
                                if restricted_paths.is_file {
                                    None
                                } else {
                                    let parent_abs_path =
                                        restricted_paths.abs_path.parent()?.to_owned();
                                    Some(PathTrust::AbsPath(parent_abs_path))
                                }
                            },
                        ));
                    }
                }
                trusted_worktrees.trust(&worktree_store, paths_to_trust, cx);
            });
        }

        self.trusted = Some(true);
        self.dismiss(cx);
    }

    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn single_trustable_path(&self) -> Option<Arc<Path>> {
        let mut projects = self
            .restricted_paths
            .values()
            .filter(|restricted_path| !restricted_path.is_file)
            .map(|restricted_path| restricted_path.abs_path.clone());
        let only = projects.next()?;
        projects.next().is_none().then_some(only)
    }

    pub fn refresh_restricted_paths(&mut self, cx: &mut Context<Self>) {
        if let Some(trusted_worktrees) = TrustedWorktrees::try_get_global(cx) {
            if let Some(worktree_store) = self.worktree_store.upgrade() {
                let new_restricted_worktrees = trusted_worktrees
                    .read(cx)
                    .restricted_worktrees(&worktree_store, cx)
                    .into_iter()
                    .filter_map(|(worktree_id, abs_path)| {
                        let worktree = worktree_store.read(cx).worktree_for_id(worktree_id, cx)?;
                        Some((
                            worktree_id,
                            RestrictedPath {
                                abs_path,
                                is_file: worktree.read(cx).is_single_file(),
                                host: self.remote_host.clone(),
                            },
                        ))
                    })
                    .collect::<HashMap<_, _>>();

                if self.restricted_paths != new_restricted_worktrees {
                    self.trust_parents = false;
                    self.trust_path_error = None;
                    self.restricted_paths = new_restricted_worktrees;
                    if self.restricted_paths.is_empty() {
                        self.trusted = Some(true);
                        self.dismiss(cx);
                    } else {
                        cx.notify();
                    }
                }
            }
        } else if !self.restricted_paths.is_empty() {
            self.restricted_paths.clear();
            cx.notify();
        }
    }
}

fn validate_trust_scope(
    typed: &str,
    project: &Path,
    home_dir: Option<&Path>,
    path_style: PathStyle,
) -> Result<PathBuf, TrustScopeValidationError> {
    let trimmed = typed.trim();
    if trimmed.is_empty() {
        return Err(TrustScopeValidationError::Empty);
    }

    let expanded = match (trimmed.strip_prefix('~'), home_dir) {
        (Some(rest), Some(home_dir)) => home_dir.join(
            rest.strip_prefix(path_style.primary_separator())
                .unwrap_or(rest),
        ),
        _ => PathBuf::from(trimmed),
    };
    if !util::paths::is_absolute(&expanded.to_string_lossy(), path_style) {
        return Err(TrustScopeValidationError::NotAbsolute);
    }

    if !project.starts_with(&expanded) {
        return Err(TrustScopeValidationError::NotAncestor);
    }

    Ok(expanded)
}

#[cfg(test)]
mod tests {
    use super::{PathStyle, TrustScopeValidationError, validate_trust_scope};
    use std::path::{Path, PathBuf};

    fn sample_home_dir() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\Users\tester")
        } else {
            PathBuf::from("/root")
        }
    }

    fn sample_project_dir() -> PathBuf {
        sample_home_dir().join("projects").join("demo")
    }

    #[test]
    fn validate_trust_scope_accepts_project_ancestor() {
        let home_dir = sample_home_dir();
        let project_dir = sample_project_dir();
        let scope = home_dir.join("projects");
        let style = PathStyle::local();
        let result = validate_trust_scope(
            scope.to_string_lossy().as_ref(),
            &project_dir,
            Some(&home_dir),
            style,
        );

        assert_eq!(result, Ok(scope));
    }

    #[test]
    fn validate_trust_scope_expands_home_and_rejects_non_ancestor() {
        let home_dir = sample_home_dir();
        let project_dir = sample_project_dir();
        let style = PathStyle::local();
        let home_relative_scope = format!("~{}projects", style.primary_separator());
        let ok = validate_trust_scope(&home_relative_scope, &project_dir, Some(&home_dir), style);
        assert_eq!(ok, Ok(home_dir.join("projects")));

        let err = validate_trust_scope(
            home_dir.join("elsewhere").to_string_lossy().as_ref(),
            &project_dir,
            Some(&home_dir),
            style,
        );
        assert_eq!(err, Err(TrustScopeValidationError::NotAncestor));
    }

    #[test]
    fn validate_trust_scope_accepts_remote_posix_paths() {
        let project = Path::new("/Users/me/dev/delta/wt/t1");
        let home = Path::new("/Users/me");
        let style = PathStyle::Posix;
        assert_eq!(
            validate_trust_scope("/Users/me/dev/delta/wt", project, None, style).unwrap(),
            PathBuf::from("/Users/me/dev/delta/wt"),
        );
        assert_eq!(
            validate_trust_scope("~/dev/delta/wt", project, Some(home), style).unwrap(),
            PathBuf::from("/Users/me/dev/delta/wt"),
        );
        assert_eq!(
            validate_trust_scope("/Users/me/dev/delta/wt/t1", project, None, style).unwrap(),
            PathBuf::from("/Users/me/dev/delta/wt/t1"),
        );
        assert!(validate_trust_scope("/Users/me/dev", project, None, style).is_ok());
    }

    #[test]
    fn validate_trust_scope_rejects_remote_non_ancestor_or_relative_paths() {
        let project = Path::new("/Users/me/dev/delta/wt/t1");
        let style = PathStyle::Posix;
        assert_eq!(
            validate_trust_scope("/Users/other", project, None, style),
            Err(TrustScopeValidationError::NotAncestor)
        );
        assert_eq!(
            validate_trust_scope("relative/path", project, None, style),
            Err(TrustScopeValidationError::NotAbsolute)
        );
        assert_eq!(
            validate_trust_scope("   ", project, None, style),
            Err(TrustScopeValidationError::Empty)
        );
        assert_eq!(
            validate_trust_scope("/Users/me/dev/delta/wt/t1/sub", project, None, style),
            Err(TrustScopeValidationError::NotAncestor)
        );
    }

    #[test]
    fn validate_trust_scope_expands_posix_home_root() {
        let home = Path::new("/Users/me");
        let project = Path::new("/Users/me/dev/wt/t1");
        let style = PathStyle::Posix;
        assert_eq!(
            validate_trust_scope("~", project, Some(home), style).unwrap(),
            PathBuf::from("/Users/me"),
        );
    }
}
