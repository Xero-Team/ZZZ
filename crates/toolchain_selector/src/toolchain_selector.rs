mod active_toolchain;

pub use active_toolchain::ActiveToolchain;
use anyhow::Context as _;
use convert_case::Casing as _;
use editor::Editor;
use futures::channel::oneshot;
use fuzzy::{StringMatch, StringMatchCandidate, match_strings};
use gpui::{
    Action, Animation, AnimationExt, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle,
    Focusable, KeyContext, ParentElement, Render, Styled, Subscription, Task, WeakEntity, Window,
    actions, pulsating_between,
};
use i18n::tr;
use language::{Language, LanguageName, Toolchain, ToolchainScope};
use open_path_prompt::OpenPathDelegate;
use picker::{Picker, PickerDelegate};
use project::{DirectoryLister, Project, ProjectPath, Toolchains, WorktreeId};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use ui::{
    Divider, HighlightedLabel, KeyBinding, List, ListItem, ListItemSpacing, Navigable,
    NavigableEntry, prelude::*,
};
use util::{ResultExt, maybe, paths::PathStyle, rel_path::RelPath};
use workspace::{ModalView, Workspace};

actions!(
    toolchain,
    [
        /// Selects a toolchain for the current project.
        Select,
        /// Adds a new toolchain for the current project.
        AddToolchain
    ]
);

pub fn init(cx: &mut App) {
    cx.observe_new(ToolchainSelector::register).detach();
}

pub struct ToolchainSelector {
    state: State,
    create_search_state: Arc<dyn Fn(&mut Window, &mut Context<Self>) -> SearchState + 'static>,
    language: Option<Arc<Language>>,
    project: Entity<Project>,
    language_name: LanguageName,
    worktree_id: WorktreeId,
    relative_path: Arc<RelPath>,
}

#[derive(Clone)]
struct SearchState {
    picker: Entity<Picker<ToolchainSelectorDelegate>>,
}

struct AddToolchainState {
    state: AddState,
    project: Entity<Project>,
    language_name: LanguageName,
    root_path: ProjectPath,
    weak: WeakEntity<ToolchainSelector>,
    worktree_root_path: Arc<Path>,
}

struct ScopePickerState {
    entries: [NavigableEntry; 3],
    selected_scope: ToolchainScope,
}

#[expect(
    dead_code,
    reason = "These tasks have to be kept alive to run to completion"
)]
enum PathInputState {
    WaitingForPath(Task<()>),
    Resolving(Task<()>),
}

enum AddState {
    Path {
        picker: Entity<Picker<open_path_prompt::OpenPathDelegate>>,
        error: Option<Arc<str>>,
        input_state: PathInputState,
        _subscription: Subscription,
    },
    Name {
        toolchain: Toolchain,
        editor: Entity<Editor>,
        scope_picker: ScopePickerState,
    },
}

impl AddToolchainState {
    fn new(
        project: Entity<Project>,
        language_name: LanguageName,
        root_path: ProjectPath,
        window: &mut Window,
        cx: &mut Context<ToolchainSelector>,
    ) -> anyhow::Result<Entity<Self>> {
        let weak = cx.weak_entity();
        let worktree_root_path = project
            .read(cx)
            .worktree_for_id(root_path.worktree_id, cx)
            .map(|worktree| worktree.read(cx).abs_path())
            .context("Could not find worktree")?;
        Ok(cx.new(|cx| {
            let (lister, rx) = Self::create_path_browser_delegate(project.clone(), cx);
            let path_style = project.read(cx).path_style(cx);
            let picker = cx.new(|cx| {
                let picker = Picker::uniform_list(lister, window, cx);
                let mut worktree_root = worktree_root_path.to_string_lossy().into_owned();
                worktree_root.push_str(path_style.primary_separator());
                picker.set_query(&worktree_root, window, cx);
                picker
            });

            Self {
                state: AddState::Path {
                    _subscription: cx.subscribe(&picker, |_, _, _: &DismissEvent, cx| {
                        cx.stop_propagation();
                    }),
                    picker,
                    error: None,
                    input_state: Self::wait_for_path(rx, window, cx),
                },
                project,
                language_name,
                root_path,
                weak,
                worktree_root_path,
            }
        }))
    }

    fn create_path_browser_delegate(
        project: Entity<Project>,
        cx: &mut Context<Self>,
    ) -> (OpenPathDelegate, oneshot::Receiver<Option<Vec<PathBuf>>>) {
        let (tx, rx) = oneshot::channel();
        let weak = cx.weak_entity();
        let lister = OpenPathDelegate::new(tx, DirectoryLister::Project(project), false, cx)
            .show_hidden()
            .with_footer(Arc::new(move |_, cx| {
                let error = weak
                    .read_with(cx, |this, _| {
                        if let AddState::Path { error, .. } = &this.state {
                            error.clone()
                        } else {
                            None
                        }
                    })
                    .log_err()
                    .flatten();
                let is_loading = weak
                    .read_with(cx, |this, _| {
                        matches!(
                            this.state,
                            AddState::Path {
                                input_state: PathInputState::Resolving(_),
                                ..
                            }
                        )
                    })
                    .unwrap_or_default();
                Some(
                    v_flex()
                        .child(Divider::horizontal())
                        .child(
                            h_flex()
                                .p_1()
                                .justify_between()
                                .gap_2()
                                .child(
                                    Label::new(tr(
                                        cx,
                                        "toolchain_selector.select_toolchain_path",
                                        "Select Toolchain Path",
                                    ))
                                    .color(Color::Muted)
                                    .map(|this| {
                                        if is_loading {
                                            this.with_animation(
                                                "select-toolchain-label",
                                                Animation::new(Duration::from_secs(2))
                                                    .repeat()
                                                    .with_easing(pulsating_between(0.4, 0.8)),
                                                |label, delta| label.alpha(delta),
                                            )
                                            .into_any()
                                        } else {
                                            this.into_any_element()
                                        }
                                    }),
                                )
                                .when_some(error, |this, error| {
                                    this.child(Label::new(error).color(Color::Error))
                                }),
                        )
                        .into_any(),
                )
            }));

        (lister, rx)
    }
    fn resolve_path(
        path: PathBuf,
        root_path: ProjectPath,
        language_name: LanguageName,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> PathInputState {
        PathInputState::Resolving(cx.spawn_in(window, async move |this, cx| {
            _ = maybe!(async move {
                let toolchain = match project.read_with(cx, |project, cx| {
                    project.native_path_for_execution(&path, cx)
                }) {
                    Ok(native_path) => {
                        project
                            .update(cx, |this, cx| {
                                this.resolve_toolchain(native_path, language_name, cx)
                            })
                            .await
                    }
                    Err(error) => Err(error),
                };
                let toolchain = match toolchain {
                    Ok(toolchain) => toolchain,
                    Err(error) => {
                        let message: Arc<str> = Arc::from(error.to_string());
                        // Go back to the path input state.
                        this.update_in(cx, move |this, window, cx| {
                            if let AddState::Path {
                                input_state,
                                picker,
                                error,
                                ..
                            } = &mut this.state
                                && matches!(input_state, PathInputState::Resolving(_))
                            {
                                *error = Some(message);
                                let (delegate, rx) =
                                    Self::create_path_browser_delegate(this.project.clone(), cx);
                                picker.update(cx, |picker, cx| {
                                    *picker = Picker::uniform_list(delegate, window, cx);
                                    picker.set_query(path.to_string_lossy().as_ref(), window, cx);
                                });
                                *input_state = Self::wait_for_path(rx, window, cx);
                                this.focus_handle(cx).focus(window, cx);
                            }
                        })
                        .log_err();
                        return Err(anyhow::anyhow!("Failed to resolve toolchain"));
                    }
                };
                let resolved_toolchain_path = project.read_with(cx, |this, cx| {
                    this.find_project_path_for_native_path(&toolchain.native_path, cx)
                });

                // Suggest a default scope based on the applicability.
                let scope = if let Some(project_path) = resolved_toolchain_path {
                    if !root_path.path.as_ref().is_empty() && project_path.starts_with(&root_path) {
                        let worktree_root_path = project
                            .read_with(cx, |this, cx| {
                                this.worktree_for_id(root_path.worktree_id, cx)
                                    .map(|worktree| worktree.read(cx).abs_path())
                            })
                            .context("Could not find a worktree with a given worktree ID")?;
                        ToolchainScope::Subproject(worktree_root_path, root_path.path)
                    } else {
                        ToolchainScope::Project
                    }
                } else {
                    // This path lies outside of the project.
                    ToolchainScope::Global
                };

                this.update_in(cx, |this, window, cx| {
                    let scope_picker = ScopePickerState {
                        entries: std::array::from_fn(|_| NavigableEntry::focusable(cx)),
                        selected_scope: scope,
                    };
                    this.state = AddState::Name {
                        editor: cx.new(|cx| {
                            let mut editor = Editor::single_line(window, cx);
                            editor.set_text(toolchain.name.as_ref(), window, cx);
                            editor
                        }),
                        toolchain,
                        scope_picker,
                    };
                    this.focus_handle(cx).focus(window, cx);
                })
                .log_err();

                Result::<_, anyhow::Error>::Ok(())
            })
            .await;
        }))
    }

    fn wait_for_path(
        rx: oneshot::Receiver<Option<Vec<PathBuf>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> PathInputState {
        let task = cx.spawn_in(window, async move |this, cx| {
            maybe!(async move {
                let result = rx.await.log_err()?;

                let path = result
                    .into_iter()
                    .flat_map(|paths| paths.into_iter())
                    .next()?;
                this.update_in(cx, |this, window, cx| {
                    if let AddState::Path {
                        input_state, error, ..
                    } = &mut this.state
                        && matches!(input_state, PathInputState::WaitingForPath(_))
                    {
                        error.take();
                        *input_state = Self::resolve_path(
                            path,
                            this.root_path.clone(),
                            this.language_name.clone(),
                            this.project.clone(),
                            window,
                            cx,
                        );
                    }
                })
                .log_err()?;
                Some(())
            })
            .await;
        });
        PathInputState::WaitingForPath(task)
    }

    fn confirm_toolchain(
        &mut self,
        _: &menu::Confirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let AddState::Name {
            toolchain,
            editor,
            scope_picker,
        } = &mut self.state
        else {
            return;
        };

        let text = editor.read(cx).text(cx);
        if text.is_empty() {
            return;
        }

        toolchain.name = SharedString::from(text);
        self.project.update(cx, |this, cx| {
            this.add_toolchain(toolchain.clone(), scope_picker.selected_scope.clone(), cx);
        });
        self.weak
            .update(cx, |this, cx| {
                this.state = State::Search((this.create_search_state)(window, cx));
                this.focus_handle(cx).focus(window, cx);
                cx.notify();
            })
            .log_err();
    }
}
impl Focusable for AddToolchainState {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match &self.state {
            AddState::Path { picker, .. } => picker.focus_handle(cx),
            AddState::Name { editor, .. } => editor.focus_handle(cx),
        }
    }
}

impl AddToolchainState {
    fn select_scope(&mut self, scope: ToolchainScope, cx: &mut Context<Self>) {
        if let AddState::Name { scope_picker, .. } = &mut self.state {
            scope_picker.selected_scope = scope;
            cx.notify();
        }
    }
}

impl Focusable for State {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self {
            State::Search(state) => state.picker.focus_handle(cx),
            State::AddToolchain(state) => state.focus_handle(cx),
        }
    }
}
impl Render for AddToolchainState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let weak = self.weak.upgrade();
        let label: SharedString = tr(cx, "toolchain_selector.add", "Add").into();

        v_flex()
            .size_full()
            // todo: These modal styles shouldn't be needed as the modal picker already has `elevation_3`
            // They get duplicated in the middle state of adding a virtual env, but then are needed for this last state
            .bg(cx.theme().colors().elevated_surface_background)
            .border_1()
            .border_color(cx.theme().colors().border_variant)
            .rounded_lg()
            .when_some(weak, |this, weak| {
                this.on_action(window.listener_for(
                    &weak,
                    |this: &mut ToolchainSelector, _: &menu::Cancel, window, cx| {
                        this.state = State::Search((this.create_search_state)(window, cx));
                        this.state.focus_handle(cx).focus(window, cx);
                        cx.notify();
                    },
                ))
            })
            .on_action(cx.listener(Self::confirm_toolchain))
            .map(|this| match &self.state {
                AddState::Path { picker, .. } => this.child(picker.clone()),
                AddState::Name {
                    editor,
                    scope_picker,
                    ..
                } => {
                    let scope_options = [
                        ToolchainScope::Global,
                        ToolchainScope::Project,
                        ToolchainScope::Subproject(
                            self.worktree_root_path.clone(),
                            self.root_path.path.clone(),
                        ),
                    ];

                    let mut navigable_scope_picker = Navigable::new(
                        v_flex()
                            .child(
                                h_flex()
                                    .w_full()
                                    .p_2()
                                    .border_b_1()
                                    .border_color(theme.colors().border)
                                    .child(editor.clone()),
                            )
                            .child(
                                v_flex()
                                    .child(
                                        Label::new(tr(cx, "toolchain_selector.scope", "Scope"))
                                            .size(LabelSize::Small)
                                            .color(Color::Muted)
                                            .mt_1()
                                            .ml_2(),
                                    )
                                    .child(List::new().children(
                                        scope_options.iter().enumerate().map(|(i, scope)| {
                                            let is_selected = *scope == scope_picker.selected_scope;
                                            let label = scope.label();
                                            let description = scope.description();
                                            let scope_clone_for_action = scope.clone();
                                            let scope_clone_for_click = scope.clone();

                                            div()
                                                .id(SharedString::from(format!("scope-option-{i}")))
                                                .track_focus(&scope_picker.entries[i].focus_handle)
                                                .on_action(cx.listener(
                                                    move |this, _: &menu::Confirm, _, cx| {
                                                        this.select_scope(
                                                            scope_clone_for_action.clone(),
                                                            cx,
                                                        );
                                                    },
                                                ))
                                                .child(
                                                    ListItem::new(SharedString::from(format!(
                                                        "scope-{i}"
                                                    )))
                                                    .toggle_state(
                                                        is_selected
                                                            || scope_picker.entries[i]
                                                                .focus_handle
                                                                .contains_focused(window, cx),
                                                    )
                                                    .inset(true)
                                                    .spacing(ListItemSpacing::Sparse)
                                                    .child(
                                                        h_flex()
                                                            .gap_2()
                                                            .child(Label::new(label))
                                                            .child(
                                                                Label::new(description)
                                                                    .size(LabelSize::Small)
                                                                    .color(Color::Muted),
                                                            ),
                                                    )
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.select_scope(
                                                            scope_clone_for_click.clone(),
                                                            cx,
                                                        );
                                                    })),
                                                )
                                        }),
                                    ))
                                    .child(Divider::horizontal())
                                    .child(h_flex().p_1p5().justify_end().map(|this| {
                                        let is_disabled = editor.read(cx).is_empty(cx);
                                        let handle = self.focus_handle(cx);
                                        this.child(
                                            Button::new("add-toolchain", label)
                                                .disabled(is_disabled)
                                                .key_binding(KeyBinding::for_action_in(
                                                    &menu::Confirm,
                                                    &handle,
                                                    cx,
                                                ))
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.confirm_toolchain(
                                                        &menu::Confirm,
                                                        window,
                                                        cx,
                                                    );
                                                }))
                                        )
                                    })),
                            )
                            .into_any_element(),
                    );

                    for entry in &scope_picker.entries {
                        navigable_scope_picker = navigable_scope_picker.entry(entry.clone());
                    }

                    this.child(navigable_scope_picker.render(window, cx))
                }
            })
    }
}

#[derive(Clone)]
enum State {
    Search(SearchState),
    AddToolchain(Entity<AddToolchainState>),
}

impl RenderOnce for State {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        match self {
            State::Search(state) => state.picker.into_any_element(),
            State::AddToolchain(state) => state.into_any_element(),
        }
    }
}
impl ToolchainSelector {
    fn register(
        workspace: &mut Workspace,
        _window: Option<&mut Window>,
        _: &mut Context<Workspace>,
    ) {
        workspace.register_action(move |workspace, _: &Select, window, cx| {
            Self::toggle(workspace, window, cx);
        });
        workspace.register_action(move |workspace, _: &AddToolchain, window, cx| {
            let Some(toolchain_selector) = workspace.active_modal::<Self>(cx) else {
                Self::toggle(workspace, window, cx);
                return;
            };

            toolchain_selector.update(cx, |toolchain_selector, cx| {
                toolchain_selector.handle_add_toolchain(&AddToolchain, window, cx);
            });
        });
    }

    fn toggle(
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Option<()> {
        let buffer = workspace
            .active_item(cx)?
            .act_as::<Editor>(cx)?
            .read(cx)
            .active_buffer(cx)?;
        let project = workspace.project().clone();

        let language_name = buffer.read(cx).language()?.name();
        let worktree_id = buffer.read(cx).file()?.worktree_id(cx);
        let relative_path: Arc<RelPath> = buffer.read(cx).file()?.path().parent()?.into();
        let worktree_root_path = project
            .read(cx)
            .worktree_for_id(worktree_id, cx)?
            .read(cx)
            .abs_path();
        let weak = workspace.weak_handle();
        cx.spawn_in(window, async move |workspace, cx| {
            let active_toolchain = project
                .read_with(cx, |this, cx| {
                    this.active_toolchain(
                        ProjectPath {
                            worktree_id,
                            path: relative_path.clone(),
                        },
                        language_name.clone(),
                        cx,
                    )
                })
                .await;
            workspace
                .update_in(cx, |this, window, cx| {
                    this.toggle_modal(window, cx, move |window, cx| {
                        ToolchainSelector::new(
                            weak,
                            project,
                            active_toolchain,
                            worktree_id,
                            worktree_root_path,
                            relative_path,
                            language_name,
                            window,
                            cx,
                        )
                    });
                })
                .log_err();
            anyhow::Ok(())
        })
        .detach();

        Some(())
    }

    fn new(
        workspace: WeakEntity<Workspace>,
        project: Entity<Project>,
        active_toolchain: Option<Toolchain>,
        worktree_id: WorktreeId,
        worktree_root: Arc<Path>,
        relative_path: Arc<RelPath>,
        language_name: LanguageName,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let language_registry = project.read(cx).languages().clone();
        cx.spawn({
            let language_name = language_name.clone();
            async move |this, cx| {
                let language = language_registry
                    .language_for_name(&language_name.0)
                    .await
                    .log_err();
                this.update(cx, |this, cx| {
                    this.language = language;
                    cx.notify();
                })
                .log_err();
            }
        })
        .detach();
        let project_clone = project.clone();
        let language_name_clone = language_name.clone();
        let relative_path_clone = relative_path.clone();

        let create_search_state = Arc::new(move |window: &mut Window, cx: &mut Context<Self>| {
            let toolchain_selector = cx.entity().downgrade();
            let picker = cx.new(|cx| {
                let delegate = ToolchainSelectorDelegate::new(
                    active_toolchain.clone(),
                    toolchain_selector,
                    workspace.clone(),
                    worktree_id,
                    worktree_root.clone(),
                    project_clone.clone(),
                    relative_path_clone.clone(),
                    language_name_clone.clone(),
                    window,
                    cx,
                );
                Picker::uniform_list(delegate, window, cx)
            });
            let picker_focus_handle = picker.focus_handle(cx);
            picker.update(cx, |picker, _| {
                picker.delegate.focus_handle = picker_focus_handle.clone();
            });
            SearchState { picker }
        });

        Self {
            state: State::Search(create_search_state(window, cx)),
            create_search_state,
            language: None,
            project,
            language_name,
            worktree_id,
            relative_path,
        }
    }

    fn handle_add_toolchain(
        &mut self,
        _: &AddToolchain,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(self.state, State::Search(_)) {
            let Ok(state) = AddToolchainState::new(
                self.project.clone(),
                self.language_name.clone(),
                ProjectPath {
                    worktree_id: self.worktree_id,
                    path: self.relative_path.clone(),
                },
                window,
                cx,
            ) else {
                return;
            };
            self.state = State::AddToolchain(state);
            self.state.focus_handle(cx).focus(window, cx);
            cx.notify();
        }
    }
}

impl Render for ToolchainSelector {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut key_context = KeyContext::new_with_defaults();
        key_context.add("ToolchainSelector");

        v_flex()
            .key_context(key_context)
            .w(rems(34.))
            .on_action(cx.listener(Self::handle_add_toolchain))
            .child(self.state.clone().render(window, cx))
    }
}

impl Focusable for ToolchainSelector {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.state.focus_handle(cx)
    }
}

impl EventEmitter<DismissEvent> for ToolchainSelector {}
impl ModalView for ToolchainSelector {}

pub struct ToolchainSelectorDelegate {
    toolchain_selector: WeakEntity<ToolchainSelector>,
    candidates: Arc<[(Toolchain, Option<ToolchainScope>)]>,
    matches: Vec<StringMatch>,
    selected_index: usize,
    workspace: WeakEntity<Workspace>,
    worktree_id: WorktreeId,
    worktree_abs_path_root: Arc<Path>,
    relative_path: Arc<RelPath>,
    placeholder_text: Arc<str>,
    add_toolchain_text: Arc<str>,
    project: Entity<Project>,
    focus_handle: FocusHandle,
    _fetch_candidates_task: Task<Option<()>>,
}

impl ToolchainSelectorDelegate {
    fn new(
        active_toolchain: Option<Toolchain>,
        toolchain_selector: WeakEntity<ToolchainSelector>,
        workspace: WeakEntity<Workspace>,
        worktree_id: WorktreeId,
        worktree_abs_path_root: Arc<Path>,
        project: Entity<Project>,
        relative_path: Arc<RelPath>,
        language_name: LanguageName,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Self {
        let detection_project = project.clone();
        let path_style = project.read(cx).path_style(cx);

        let _fetch_candidates_task = cx.spawn_in(window, {
            async move |this, cx| {
                let meta = detection_project
                    .read_with(cx, |this, _| {
                        Project::toolchain_metadata(this.languages().clone(), language_name.clone())
                    })
                    .await?;
                let requested_path = this
                    .update(cx, |this, cx| {
                        this.delegate.add_toolchain_text = format!(
                            "{} {}",
                            tr(cx, "toolchain_selector.add", "Add"),
                            meta.term.as_ref().to_case(convert_case::Case::Title)
                        )
                        .into();
                        cx.notify();
                        this.delegate.relative_path.clone()
                    })
                    .log_err()?;

                // `available_toolchains` reports the path it actually ran the
                // detection at, which can differ from the requested one.
                let Toolchains {
                    toolchains: available_toolchains,
                    root_path,
                    user_toolchains,
                } = detection_project
                    .update(cx, |this, cx| {
                        this.available_toolchains(
                            ProjectPath {
                                worktree_id,
                                path: requested_path,
                            },
                            language_name,
                            cx,
                        )
                    })
                    .await?;
                let worktree_root_label = this
                    .update(cx, |_, cx| {
                        tr(cx, "toolchain_selector.worktree_root", "worktree root")
                    })
                    .log_err()?;
                let pretty_path = if root_path.is_empty() {
                    worktree_root_label
                } else {
                    format!("`{}`", root_path.display(path_style))
                };
                let placeholder_text = this
                    .update(cx, |_, cx| {
                        tr(
                            cx,
                            "toolchain_selector.select_for_path",
                            "Select a {} for {}...",
                        )
                        .replacen("{}", &meta.term.to_lowercase(), 1)
                        .replacen("{}", &pretty_path, 1)
                    })
                    .log_err()?;
                this.update_in(cx, move |this, window, cx| {
                    this.delegate.relative_path = root_path;
                    this.delegate.placeholder_text = placeholder_text.into();
                    this.refresh_placeholder(window, cx);
                })
                .log_err();

                this.update_in(cx, move |this, window, cx| {
                    this.delegate.candidates = user_toolchains
                        .into_iter()
                        .flat_map(|(scope, toolchains)| {
                            toolchains
                                .into_iter()
                                .map(move |toolchain| (toolchain, Some(scope.clone())))
                        })
                        .chain(
                            available_toolchains
                                .toolchains
                                .into_iter()
                                .map(|toolchain| (toolchain, None)),
                        )
                        .collect();

                    if let Some(active_toolchain) = active_toolchain
                        && let Some(position) = this
                            .delegate
                            .candidates
                            .iter()
                            .position(|(toolchain, _)| *toolchain == active_toolchain)
                    {
                        this.delegate.set_selected_index(position, window, cx);
                    }
                    this.update_matches(this.query(cx), window, cx);
                })
                .log_err();

                Some(())
            }
        });
        let placeholder_text = tr(
            cx,
            "toolchain_selector.placeholder",
            "Select a toolchain...",
        )
        .into();
        Self {
            toolchain_selector,
            candidates: Default::default(),
            matches: vec![],
            selected_index: 0,
            workspace,
            worktree_id,
            worktree_abs_path_root,
            placeholder_text,
            relative_path,
            _fetch_candidates_task,
            project,
            focus_handle: cx.focus_handle(),
            add_toolchain_text: Arc::from(tr(
                cx,
                "toolchain_selector.add_toolchain",
                "Add Toolchain",
            )),
        }
    }
    /// Removes the candidate at `candidate_id` and every match pointing at it.
    ///
    /// The candidate ids stored in the matches are indices into
    /// `self.candidates`, so every id above the removed one has to be shifted
    /// down to keep the two collections aligned.
    fn remove_candidate(&mut self, candidate_id: usize) {
        if candidate_id >= self.candidates.len() {
            return;
        }

        let dropped_before_selection =
            remove_matches_for_candidate(&mut self.matches, candidate_id, self.selected_index);

        let candidates = self
            .candidates
            .iter()
            .enumerate()
            .filter_map(|(ix, candidate)| (ix != candidate_id).then(|| candidate.clone()))
            .collect();
        self.candidates = candidates;

        self.selected_index = self
            .selected_index
            .saturating_sub(dropped_before_selection)
            .min(self.matches.len().saturating_sub(1));
    }

    fn relativize_path(path: String, worktree_root: &Path, path_style: PathStyle) -> SharedString {
        let relative_path = Path::new(path.as_str())
            .strip_prefix(&worktree_root)
            .ok()
            .and_then(|suffix| suffix.to_str());
        if let Some(relative_path) = relative_path {
            format!(".{}{relative_path}", path_style.primary_separator()).into()
        } else {
            path.into()
        }
    }
}

/// Drops every match pointing at `candidate_id` and shifts the ids of the later
/// candidates down by one so they keep pointing at the right entries.
///
/// Returns how many matches were dropped from before `selected_index`, which is
/// the amount the selection has to move up to stay on the same row.
fn remove_matches_for_candidate(
    matches: &mut Vec<StringMatch>,
    candidate_id: usize,
    selected_index: usize,
) -> usize {
    let mut dropped_before_selection = 0;
    let mut index = 0;
    matches.retain_mut(|candidate_match| {
        let keep = candidate_match.candidate_id != candidate_id;
        if keep && candidate_match.candidate_id > candidate_id {
            candidate_match.candidate_id -= 1;
        }
        if !keep && index < selected_index {
            dropped_before_selection += 1;
        }
        index += 1;
        keep
    });
    dropped_before_selection
}

impl PickerDelegate for ToolchainSelectorDelegate {
    type ListItem = ListItem;

    fn placeholder_text(&self, _window: &mut Window, _cx: &mut App) -> Arc<str> {
        self.placeholder_text.clone()
    }

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn confirm(&mut self, _: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        if let Some(string_match) = self.matches.get(self.selected_index)
            && let Some((toolchain, _)) = self.candidates.get(string_match.candidate_id).cloned()
            && let Some(workspace_id) = self
                .workspace
                .read_with(cx, |this, _| this.database_id())
                .log_err()
                .flatten()
        {
            let workspace = self.workspace.clone();
            let worktree_id = self.worktree_id;
            let worktree_abs_path_root = self.worktree_abs_path_root.clone();
            let path = self.relative_path.clone();
            let db = workspace::WorkspaceDb::global(cx);
            cx.spawn_in(window, async move |_, cx| {
                db.set_toolchain(
                    workspace_id,
                    worktree_abs_path_root,
                    path.clone(),
                    toolchain.clone(),
                )
                .await
                .log_err();
                workspace
                    .update(cx, |this, cx| {
                        this.project().update(cx, |this, cx| {
                            this.activate_toolchain(
                                ProjectPath { worktree_id, path },
                                toolchain,
                                cx,
                            )
                        })
                    })
                    .log_err()?
                    .await;
                Some(())
            })
            .detach();
        }
        self.dismissed(window, cx);
    }

    fn dismissed(&mut self, _: &mut Window, cx: &mut Context<Picker<Self>>) {
        self.toolchain_selector
            .update(cx, |_, cx| cx.emit(DismissEvent))
            .log_err();
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(
        &mut self,
        ix: usize,
        _window: &mut Window,
        _: &mut Context<Picker<Self>>,
    ) {
        self.selected_index = ix;
    }

    fn update_matches(
        &mut self,
        query: String,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> gpui::Task<()> {
        let background = cx.background_executor().clone();
        let candidates = self.candidates.clone();
        let worktree_root_path = self.worktree_abs_path_root.clone();
        let path_style = self.project.read(cx).path_style(cx);
        cx.spawn_in(window, async move |this, cx| {
            // The candidate id of a match indexes `self.candidates`, so the
            // rendered string is built once per candidate for both branches.
            let candidate_strings = candidates
                .iter()
                .enumerate()
                .map(|(candidate_id, (toolchain, _))| {
                    let path = Self::relativize_path(
                        toolchain.display_path(),
                        &worktree_root_path,
                        path_style,
                    );
                    (candidate_id, format!("{}{}", toolchain.name, path))
                })
                .collect::<Vec<_>>();

            let matches = if query.is_empty() {
                candidate_strings
                    .into_iter()
                    .map(|(candidate_id, string)| StringMatch {
                        candidate_id,
                        string,
                        positions: Vec::new(),
                        score: 0.0,
                    })
                    .collect()
            } else {
                let candidates = candidate_strings
                    .into_iter()
                    .map(|(candidate_id, string)| StringMatchCandidate::new(candidate_id, &string))
                    .collect::<Vec<_>>();
                match_strings(
                    &candidates,
                    &query,
                    false,
                    true,
                    100,
                    &Default::default(),
                    background,
                )
                .await
            };

            this.update(cx, |this, cx| {
                let delegate = &mut this.delegate;
                delegate.matches = matches;
                delegate.selected_index = delegate
                    .selected_index
                    .min(delegate.matches.len().saturating_sub(1));
                cx.notify();
            })
            .log_err();
        })
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        _: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let mat = &self.matches.get(ix)?;
        let (toolchain, scope) = &self.candidates.get(mat.candidate_id)?;

        let label = toolchain.name.clone();
        let path_style = self.project.read(cx).path_style(cx);
        let path = Self::relativize_path(
            toolchain.display_path(),
            &self.worktree_abs_path_root,
            path_style,
        );
        let (name_highlights, mut path_highlights) = mat
            .positions
            .iter()
            .cloned()
            .partition::<Vec<_>, _>(|index| *index < label.len());
        for index in &mut path_highlights {
            *index -= label.len();
        }
        let id: SharedString = format!("toolchain-{ix}").into();
        Some(
            ListItem::new(id)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .toggle_state(selected)
                .child(HighlightedLabel::new(label, name_highlights))
                .child(
                    HighlightedLabel::new(path, path_highlights)
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .when_some(scope.as_ref(), |this, scope| {
                    let id: SharedString = format!(
                        "delete-custom-toolchain-{}-{}",
                        toolchain.name,
                        toolchain.display_path()
                    )
                    .into();
                    // `ix` is the index into the matches, while the candidate
                    // that has to be deleted is the one this match points at.
                    let candidate_id = mat.candidate_id;
                    let toolchain = toolchain.clone();
                    let scope = scope.clone();

                    this.end_slot(IconButton::new(id, IconName::Trash).on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.delegate.project.update(cx, |this, cx| {
                                this.remove_toolchain(toolchain.clone(), scope.clone(), cx)
                            });

                            this.delegate.remove_candidate(candidate_id);
                            cx.stop_propagation();
                            cx.notify();
                        },
                    )))
                }),
        )
    }
    fn render_footer(
        &self,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<AnyElement> {
        Some(
            v_flex()
                .rounded_b_md()
                .child(Divider::horizontal())
                .child(
                    h_flex()
                        .p_1p5()
                        .gap_0p5()
                        .justify_end()
                        .child(
                            Button::new("add-toolchain", self.add_toolchain_text.clone())
                                .key_binding(KeyBinding::for_action_in(
                                    &AddToolchain,
                                    &self.focus_handle,
                                    cx,
                                ))
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(AddToolchain), cx)
                                }),
                        )
                        .child(
                            Button::new("select", tr(cx, "toolchain_selector.select", "Select"))
                                .key_binding(KeyBinding::for_action_in(
                                    &menu::Confirm,
                                    &self.focus_handle,
                                    cx,
                                ))
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(menu::Confirm.boxed_clone(), cx)
                                }),
                        ),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string_match(candidate_id: usize) -> StringMatch {
        StringMatch {
            candidate_id,
            string: format!("candidate-{candidate_id}"),
            positions: Vec::new(),
            score: 0.0,
        }
    }

    #[test]
    fn removing_a_candidate_drops_its_match_and_renumbers_the_rest() {
        // Under a fuzzy filter the match index differs from the candidate id,
        // so the removal has to go through the id carried by the match.
        let mut matches = vec![string_match(3), string_match(0), string_match(5)];

        remove_matches_for_candidate(&mut matches, 3, 0);

        let remaining = matches
            .iter()
            .map(|candidate_match| candidate_match.candidate_id)
            .collect::<Vec<_>>();
        assert_eq!(remaining, [0, 4]);
    }

    #[test]
    fn removing_an_unknown_candidate_keeps_every_match() {
        let mut matches = vec![string_match(0), string_match(1)];

        remove_matches_for_candidate(&mut matches, 7, 0);

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[1].candidate_id, 1);
    }

    #[test]
    fn the_selection_only_moves_when_a_row_above_it_disappears() {
        let mut matches = vec![string_match(0), string_match(1), string_match(2)];

        // The deleted candidate sits above the selected row, so the selection
        // has to move up to stay on the same toolchain.
        assert_eq!(remove_matches_for_candidate(&mut matches, 0, 2), 1);

        let mut matches = vec![string_match(0), string_match(1), string_match(2)];

        // The selected row itself is the deleted one: nothing above it moved.
        assert_eq!(remove_matches_for_candidate(&mut matches, 2, 2), 0);
    }
}
