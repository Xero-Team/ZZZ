use crate::tr;
mod profile_modal_header;

use std::sync::Arc;

use agent::ContextServerRegistry;
use agent_settings::{AgentProfile, AgentProfileId, AgentSettings, builtin_profiles};
use editor::Editor;
use fs::Fs;
use gpui::{DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, Subscription, prelude::*};
use settings::SettingsStore;
use settings::{Settings as _, update_settings_file};
use ui::{
    KeyBinding, ListItem, ListItemSpacing, ListSeparator, Navigable, NavigableEntry, prelude::*,
};
use workspace::{ModalView, Workspace};

use crate::agent_configuration::manage_profiles_modal::profile_modal_header::ProfileModalHeader;
use crate::agent_configuration::tool_picker::{ToolPicker, ToolPickerDelegate};
use crate::{AgentPanel, ManageProfiles};

/// A plain action row of the profile view. Every variant renders the same shape
/// of row and differs only in its icon, label and the handler it invokes.
#[derive(Clone, Copy)]
enum ProfileAction {
    ForkProfile,
    ConfigureBuiltinTools,
    ConfigureMcpTools,
    DeleteProfile,
}

/// The presentation of one [`ProfileAction`].
struct ProfileActionSpec {
    /// Element id of the focusable wrapper around the list item.
    id: &'static str,
    /// Element id of the list item itself.
    list_item_id: &'static str,
    icon: IconName,
    icon_color: Color,
    label_key: &'static str,
    label_fallback: &'static str,
    /// Colour applied to the label; `None` keeps the default label colour.
    label_color: Option<Color>,
}

impl ProfileAction {
    fn spec(self) -> ProfileActionSpec {
        match self {
            Self::ForkProfile => ProfileActionSpec {
                id: "fork-profile",
                list_item_id: "fork-profile",
                icon: IconName::Scissors,
                icon_color: Color::Muted,
                label_key: "agent_ui.manage_profiles.fork_profile",
                label_fallback: "Fork Profile",
                label_color: None,
            },
            Self::ConfigureBuiltinTools => ProfileActionSpec {
                id: "configure-builtin-tools",
                list_item_id: "configure-builtin-tools-item",
                icon: IconName::Settings,
                icon_color: Color::Muted,
                label_key: "agent_ui.manage_profiles.configure_built_in_tools",
                label_fallback: "Configure Built-in Tools",
                label_color: None,
            },
            Self::ConfigureMcpTools => ProfileActionSpec {
                id: "configure-mcps",
                list_item_id: "configure-mcp-tools",
                icon: IconName::ToolHammer,
                icon_color: Color::Muted,
                label_key: "agent_ui.manage_profiles.configure_mcp_tools",
                label_fallback: "Configure MCP Tools",
                label_color: None,
            },
            Self::DeleteProfile => ProfileActionSpec {
                id: "delete-profile",
                list_item_id: "delete-profile",
                icon: IconName::Trash,
                icon_color: Color::Error,
                label_key: "agent_ui.manage_profiles.delete_profile",
                label_fallback: "Delete Profile",
                label_color: Some(Color::Error),
            },
        }
    }

    /// Built-in profiles cannot be deleted, so their row is shown disabled.
    fn disabled_for(self, profile_id: &AgentProfileId) -> bool {
        match self {
            Self::DeleteProfile => builtin_profiles::is_builtin(profile_id),
            _ => false,
        }
    }
}

enum Mode {
    ChooseProfile(ChooseProfileMode),
    NewProfile(NewProfileMode),
    ViewProfile(ViewProfileMode),
    ConfigureTools {
        profile_id: AgentProfileId,
        tool_picker: Entity<ToolPicker>,
        _subscription: Subscription,
    },
    ConfigureMcps {
        profile_id: AgentProfileId,
        tool_picker: Entity<ToolPicker>,
        _subscription: Subscription,
    },
}

impl Mode {
    pub fn choose_profile(_window: &mut Window, cx: &mut Context<ManageProfilesModal>) -> Self {
        let settings = AgentSettings::get_global(cx);

        let mut builtin_profiles = Vec::new();
        let mut custom_profiles = Vec::new();

        for (profile_id, profile) in &settings.profiles {
            let entry = ProfileEntry {
                id: profile_id.clone(),
                name: profile.name.clone(),
                navigation: NavigableEntry::focusable(cx),
            };
            if builtin_profiles::is_builtin(profile_id) {
                builtin_profiles.push(entry);
            } else {
                custom_profiles.push(entry);
            }
        }

        builtin_profiles.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        custom_profiles.sort_unstable_by(|a, b| a.name.cmp(&b.name));

        Self::ChooseProfile(ChooseProfileMode {
            builtin_profiles,
            custom_profiles,
            add_new_profile: NavigableEntry::focusable(cx),
        })
    }
}

#[derive(Clone)]
struct ProfileEntry {
    pub id: AgentProfileId,
    pub name: SharedString,
    pub navigation: NavigableEntry,
}

#[derive(Clone)]
pub struct ChooseProfileMode {
    builtin_profiles: Vec<ProfileEntry>,
    custom_profiles: Vec<ProfileEntry>,
    add_new_profile: NavigableEntry,
}

#[derive(Clone)]
pub struct ViewProfileMode {
    profile_id: AgentProfileId,
    fork_profile: NavigableEntry,
    configure_tools: NavigableEntry,
    configure_mcps: NavigableEntry,
    delete_profile: NavigableEntry,
    cancel_item: NavigableEntry,
}

#[derive(Clone)]
pub struct NewProfileMode {
    name_editor: Entity<Editor>,
    base_profile_id: Option<AgentProfileId>,
}

pub struct ManageProfilesModal {
    fs: Arc<dyn Fs>,
    context_server_registry: Entity<ContextServerRegistry>,
    focus_handle: FocusHandle,
    mode: Mode,
    _settings_subscription: Subscription,
}

impl ManageProfilesModal {
    pub fn register(
        workspace: &mut Workspace,
        _window: Option<&mut Window>,
        _cx: &mut Context<Workspace>,
    ) {
        workspace.register_action(|workspace, action: &ManageProfiles, window, cx| {
            if let Some(panel) = workspace.panel::<AgentPanel>(cx) {
                let fs = workspace.app_state().fs.clone();

                let context_server_registry = panel.read(cx).context_server_registry().clone();
                workspace.toggle_modal(window, cx, |window, cx| {
                    let mut this = Self::new(fs, context_server_registry, window, cx);

                    if let Some(profile_id) = action.customize_tools.clone() {
                        this.configure_builtin_tools(profile_id, window, cx);
                    }

                    this
                })
            }
        });
    }

    pub fn new(
        fs: Arc<dyn Fs>,
        context_server_registry: Entity<ContextServerRegistry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();

        // Keep this modal in sync with settings changes (including profile deletion).
        let settings_subscription =
            cx.observe_global_in::<SettingsStore>(window, |this, window, cx| {
                if matches!(this.mode, Mode::ChooseProfile(_)) {
                    this.mode = Mode::choose_profile(window, cx);
                    this.focus_handle(cx).focus(window, cx);
                    cx.notify();
                }
            });

        Self {
            fs,
            context_server_registry,
            focus_handle,
            mode: Mode::choose_profile(window, cx),
            _settings_subscription: settings_subscription,
        }
    }

    fn choose_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.mode = Mode::choose_profile(window, cx);
        self.focus_handle(cx).focus(window, cx);
    }

    fn new_profile(
        &mut self,
        base_profile_id: Option<AgentProfileId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name_editor = cx.new(|cx| Editor::single_line(window, cx));
        name_editor.update(cx, |editor, cx| {
            editor.set_placeholder_text(
                &tr(
                    cx,
                    "agent_ui.manage_profiles.profile_name_placeholder",
                    "Profile name",
                ),
                window,
                cx,
            );
        });

        self.mode = Mode::NewProfile(NewProfileMode {
            name_editor,
            base_profile_id,
        });
        self.focus_handle(cx).focus(window, cx);
    }

    pub fn view_profile(
        &mut self,
        profile_id: AgentProfileId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.mode = Mode::ViewProfile(ViewProfileMode {
            profile_id,
            fork_profile: NavigableEntry::focusable(cx),
            configure_tools: NavigableEntry::focusable(cx),
            configure_mcps: NavigableEntry::focusable(cx),
            delete_profile: NavigableEntry::focusable(cx),
            cancel_item: NavigableEntry::focusable(cx),
        });
        self.focus_handle(cx).focus(window, cx);
    }

    fn configure_mcp_tools(
        &mut self,
        profile_id: AgentProfileId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let settings = AgentSettings::get_global(cx);
        let Some(profile) = settings.profiles.get(&profile_id).cloned() else {
            return;
        };

        let tool_picker = cx.new(|cx| {
            let delegate = ToolPickerDelegate::mcp_tools(
                &self.context_server_registry,
                self.fs.clone(),
                profile_id.clone(),
                profile,
                cx,
            );
            ToolPicker::mcp_tools(delegate, window, cx)
        });
        let dismiss_subscription = cx.subscribe_in(&tool_picker, window, {
            let profile_id = profile_id.clone();
            move |this, _tool_picker, _: &DismissEvent, window, cx| {
                this.view_profile(profile_id.clone(), window, cx);
            }
        });

        self.mode = Mode::ConfigureMcps {
            profile_id,
            tool_picker,
            _subscription: dismiss_subscription,
        };
        self.focus_handle(cx).focus(window, cx);
    }

    fn configure_builtin_tools(
        &mut self,
        profile_id: AgentProfileId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let settings = AgentSettings::get_global(cx);
        let Some(profile) = settings.profiles.get(&profile_id).cloned() else {
            return;
        };

        let tool_names: Vec<Arc<str>> = agent::ALL_TOOL_NAMES
            .iter()
            .copied()
            .map(Arc::from)
            .collect();

        let tool_picker = cx.new(|cx| {
            let delegate = ToolPickerDelegate::builtin_tools(
                tool_names,
                self.fs.clone(),
                profile_id.clone(),
                profile,
                cx,
            );
            ToolPicker::builtin_tools(delegate, window, cx)
        });
        let dismiss_subscription = cx.subscribe_in(&tool_picker, window, {
            let profile_id = profile_id.clone();
            move |this, _tool_picker, _: &DismissEvent, window, cx| {
                this.view_profile(profile_id.clone(), window, cx);
            }
        });

        self.mode = Mode::ConfigureTools {
            profile_id,
            tool_picker,
            _subscription: dismiss_subscription,
        };
        self.focus_handle(cx).focus(window, cx);
    }

    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match &self.mode {
            Mode::ChooseProfile { .. } => {}
            Mode::NewProfile(mode) => {
                let name = mode.name_editor.read(cx).text(cx);
                let base_profile_id = mode.base_profile_id.clone();

                let profile_id = AgentProfile::create(name, base_profile_id, self.fs.clone(), cx);
                self.view_profile(profile_id, window, cx);
            }
            Mode::ViewProfile(_) => {}
            Mode::ConfigureTools { .. } => {}
            Mode::ConfigureMcps { .. } => {}
        }
    }

    fn delete_profile(
        &mut self,
        profile_id: AgentProfileId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if builtin_profiles::is_builtin(&profile_id) {
            self.view_profile(profile_id, window, cx);
            return;
        }

        let fs = self.fs.clone();

        update_settings_file(fs, cx, move |settings, _cx| {
            let Some(agent_settings) = settings.agent.as_mut() else {
                return;
            };

            let Some(profiles) = agent_settings.profiles.as_mut() else {
                return;
            };

            profiles.shift_remove(profile_id.0.as_ref());

            if agent_settings
                .default_profile
                .as_deref()
                .is_some_and(|default_profile| default_profile == profile_id.0.as_ref())
            {
                agent_settings.default_profile = Some(AgentProfileId::default().0);
            }
        });

        self.choose_profile(window, cx);
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match &self.mode {
            Mode::ChooseProfile { .. } => {
                cx.emit(DismissEvent);
            }
            Mode::NewProfile(mode) => {
                if let Some(profile_id) = mode.base_profile_id.clone() {
                    self.view_profile(profile_id, window, cx);
                } else {
                    self.choose_profile(window, cx);
                }
            }
            Mode::ViewProfile(_) => self.choose_profile(window, cx),
            Mode::ConfigureTools { profile_id, .. } => {
                self.view_profile(profile_id.clone(), window, cx)
            }
            Mode::ConfigureMcps { profile_id, .. } => {
                self.view_profile(profile_id.clone(), window, cx)
            }
        }
    }
}

impl ModalView for ManageProfilesModal {}

impl Focusable for ManageProfilesModal {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match &self.mode {
            Mode::ChooseProfile(_) => self.focus_handle.clone(),
            Mode::NewProfile(mode) => mode.name_editor.focus_handle(cx),
            Mode::ViewProfile(_) => self.focus_handle.clone(),
            Mode::ConfigureTools { tool_picker, .. } => tool_picker.focus_handle(cx),
            Mode::ConfigureMcps { tool_picker, .. } => tool_picker.focus_handle(cx),
        }
    }
}

impl EventEmitter<DismissEvent> for ManageProfilesModal {}

impl ManageProfilesModal {
    fn render_profile(
        &self,
        profile: &ProfileEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let is_focused = profile.navigation.focus_handle.contains_focused(window, cx);
        let element_id = format!("profile-{}", profile.id);

        div()
            .id(element_id.clone())
            .track_focus(&profile.navigation.focus_handle)
            .on_action({
                let profile_id = profile.id.clone();
                cx.listener(move |this, _: &menu::Confirm, window, cx| {
                    this.view_profile(profile_id.clone(), window, cx);
                })
            })
            .child(
                ListItem::new(element_id)
                    .toggle_state(is_focused)
                    .inset(true)
                    .spacing(ListItemSpacing::Sparse)
                    .child(Label::new(profile.name.clone()))
                    .when(is_focused, |this| {
                        this.end_slot(
                            h_flex()
                                .gap_1()
                                .child(
                                    Label::new(tr(
                                        cx,
                                        "agent_ui.manage_profiles.customize",
                                        "Customize",
                                    ))
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                                )
                                .child(KeyBinding::for_action_in(
                                    &menu::Confirm,
                                    &self.focus_handle,
                                    cx,
                                )),
                        )
                    })
                    .on_click({
                        let profile_id = profile.id.clone();
                        cx.listener(move |this, _, window, cx| {
                            this.view_profile(profile_id.clone(), window, cx);
                        })
                    }),
            )
    }

    fn render_choose_profile(
        &mut self,
        mode: ChooseProfileMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        Navigable::new(
            div()
                .track_focus(&self.focus_handle(cx))
                .size_full()
                .child(ProfileModalHeader::new(
                    tr(
                        cx,
                        "agent_ui.manage_profiles.agent_profiles",
                        "Agent Profiles",
                    ),
                    None,
                ))
                .child(
                    v_flex()
                        .pb_1()
                        .child(ListSeparator)
                        .children(
                            mode.builtin_profiles
                                .iter()
                                .map(|profile| self.render_profile(profile, window, cx)),
                        )
                        .when(!mode.custom_profiles.is_empty(), |this| {
                            this.child(ListSeparator)
                                .child(
                                    div().pl_2().pb_1().child(
                                        Label::new(tr(
                                            cx,
                                            "agent_ui.manage_profiles.custom_profiles",
                                            "Custom Profiles",
                                        ))
                                        .size(LabelSize::Small)
                                        .color(Color::Muted),
                                    ),
                                )
                                .children(
                                    mode.custom_profiles
                                        .iter()
                                        .map(|profile| self.render_profile(profile, window, cx)),
                                )
                        })
                        .child(ListSeparator)
                        .child(
                            div()
                                .id("new-profile")
                                .track_focus(&mode.add_new_profile.focus_handle)
                                .on_action(cx.listener(|this, _: &menu::Confirm, window, cx| {
                                    this.new_profile(None, window, cx);
                                }))
                                .child(
                                    ListItem::new("new-profile")
                                        .toggle_state(
                                            mode.add_new_profile
                                                .focus_handle
                                                .contains_focused(window, cx),
                                        )
                                        .inset(true)
                                        .spacing(ListItemSpacing::Sparse)
                                        .start_slot(Icon::new(IconName::Plus))
                                        .child(Label::new(tr(
                                            cx,
                                            "agent_ui.manage_profiles.add_new_profile",
                                            "Add New Profile",
                                        )))
                                        .on_click({
                                            cx.listener(move |this, _, window, cx| {
                                                this.new_profile(None, window, cx);
                                            })
                                        }),
                                ),
                        ),
                )
                .into_any_element(),
        )
        .map(|mut navigable| {
            for profile in mode.builtin_profiles {
                navigable = navigable.entry(profile.navigation);
            }
            for profile in mode.custom_profiles {
                navigable = navigable.entry(profile.navigation);
            }

            navigable
        })
        .entry(mode.add_new_profile)
    }

    fn render_new_profile(
        &mut self,
        mode: NewProfileMode,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let settings = AgentSettings::get_global(cx);

        let base_profile_name = mode.base_profile_id.as_ref().map(|base_profile_id| {
            settings.profiles.get(base_profile_id).map_or_else(
                || tr(cx, "agent_ui.manage_profiles.unknown", "Unknown"),
                |profile| profile.name.clone(),
            )
        });

        v_flex()
            .id("new-profile")
            .track_focus(&self.focus_handle(cx))
            .child(ProfileModalHeader::new(
                match &base_profile_name {
                    Some(base_profile) => tr(cx, "agent_ui.manage_profiles.fork_title", "Fork {}")
                        .replacen("{}", base_profile.as_ref(), 1)
                        .into(),
                    None => tr(cx, "agent_ui.manage_profiles.new_profile", "New Profile"),
                },
                match base_profile_name {
                    Some(_) => Some(IconName::Scissors),
                    None => Some(IconName::Plus),
                },
            ))
            .child(ListSeparator)
            .child(h_flex().p_2().child(mode.name_editor))
    }

    fn invoke_profile_action(
        &mut self,
        action: ProfileAction,
        profile_id: AgentProfileId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            ProfileAction::ForkProfile => self.new_profile(Some(profile_id), window, cx),
            ProfileAction::ConfigureBuiltinTools => {
                self.configure_builtin_tools(profile_id, window, cx);
            }
            ProfileAction::ConfigureMcpTools => self.configure_mcp_tools(profile_id, window, cx),
            ProfileAction::DeleteProfile => self.delete_profile(profile_id, window, cx),
        }
    }

    /// Renders one keyboard-navigable row of the profile view. Every row shares
    /// the same shape — focusable wrapper, list item, icon, label, click and
    /// confirm handlers — and differs only in its [`ProfileActionSpec`].
    fn render_profile_action_row(
        &self,
        action: ProfileAction,
        profile_id: AgentProfileId,
        focus_handle: &FocusHandle,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let spec = action.spec();
        let is_disabled = action.disabled_for(&profile_id);

        let label = Label::new(tr(cx, spec.label_key, spec.label_fallback));
        let label = match spec.label_color {
            Some(color) => label.color(color),
            None => label,
        };

        div()
            .id(spec.id)
            .track_focus(focus_handle)
            .on_action({
                let profile_id = profile_id.clone();
                cx.listener(move |this, _: &menu::Confirm, window, cx| {
                    this.invoke_profile_action(action, profile_id.clone(), window, cx);
                })
            })
            .child(
                ListItem::new(spec.list_item_id)
                    .toggle_state(focus_handle.contains_focused(window, cx))
                    .inset(true)
                    .spacing(ListItemSpacing::Sparse)
                    .start_slot(
                        Icon::new(spec.icon)
                            .size(IconSize::Small)
                            .color(spec.icon_color),
                    )
                    .child(label)
                    .disabled(is_disabled)
                    .on_click({
                        cx.listener(move |this, _, window, cx| {
                            this.invoke_profile_action(action, profile_id.clone(), window, cx);
                        })
                    }),
            )
            .into_any_element()
    }

    /// The "Go Back" row. It is shown both at the bottom of the profile view and
    /// below the tool pickers, so it is built in exactly one place.
    fn render_go_back_row(
        &self,
        focus_handle: &FocusHandle,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .id("cancel-item")
            .track_focus(focus_handle)
            .on_action(cx.listener(|this, _: &menu::Confirm, window, cx| {
                this.cancel(window, cx);
            }))
            .child(
                ListItem::new("cancel-item")
                    .toggle_state(focus_handle.contains_focused(window, cx))
                    .inset(true)
                    .spacing(ListItemSpacing::Sparse)
                    .start_slot(
                        Icon::new(IconName::ArrowLeft)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .child(Label::new(tr(
                        cx,
                        "agent_ui.manage_profiles.go_back",
                        "Go Back",
                    )))
                    .end_slot(
                        div().child(
                            KeyBinding::for_action_in(&menu::Cancel, &self.focus_handle, cx)
                                .size(rems_from_px(12.)),
                        ),
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.cancel(window, cx);
                    })),
            )
            .into_any_element()
    }

    /// Both tool-picker modes render the same frame: a titled header, the picker
    /// itself, and the "Go Back" row.
    fn render_tool_picker_mode(
        &self,
        profile_id: &AgentProfileId,
        tool_picker: &Entity<ToolPicker>,
        header_key: &'static str,
        header_fallback: &'static str,
        icon: IconName,
        go_back_item: AnyElement,
        cx: &App,
    ) -> AnyElement {
        let profile_name = AgentSettings::get_global(cx)
            .profiles
            .get(profile_id)
            .map_or_else(
                || tr(cx, "agent_ui.manage_profiles.unknown", "Unknown"),
                |profile| profile.name.clone(),
            );

        v_flex()
            .pb_1()
            .child(ProfileModalHeader::new(
                tr(cx, header_key, header_fallback).replacen("{}", profile_name.as_ref(), 1),
                Some(icon),
            ))
            .child(ListSeparator)
            .child(tool_picker.clone())
            .child(ListSeparator)
            .child(go_back_item)
            .into_any_element()
    }

    fn render_view_profile(
        &mut self,
        mode: ViewProfileMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let settings = AgentSettings::get_global(cx);

        let profile_name = settings.profiles.get(&mode.profile_id).map_or_else(
            || tr(cx, "agent_ui.manage_profiles.unknown", "Unknown"),
            |profile| profile.name.clone(),
        );

        let icon = match mode.profile_id.as_str() {
            builtin_profiles::WRITE => IconName::Pencil,
            builtin_profiles::ASK => IconName::Chat,
            _ => IconName::UserRoundPen,
        };

        let fork_profile = self.render_profile_action_row(
            ProfileAction::ForkProfile,
            mode.profile_id.clone(),
            &mode.fork_profile.focus_handle,
            window,
            cx,
        );
        let configure_builtin_tools = self.render_profile_action_row(
            ProfileAction::ConfigureBuiltinTools,
            mode.profile_id.clone(),
            &mode.configure_tools.focus_handle,
            window,
            cx,
        );
        let configure_mcp_tools = self.render_profile_action_row(
            ProfileAction::ConfigureMcpTools,
            mode.profile_id.clone(),
            &mode.configure_mcps.focus_handle,
            window,
            cx,
        );
        let delete_profile = self.render_profile_action_row(
            ProfileAction::DeleteProfile,
            mode.profile_id.clone(),
            &mode.delete_profile.focus_handle,
            window,
            cx,
        );
        let go_back = self.render_go_back_row(&mode.cancel_item.focus_handle, window, cx);

        Navigable::new(
            div()
                .track_focus(&self.focus_handle(cx))
                .size_full()
                .child(ProfileModalHeader::new(profile_name, Some(icon)))
                .child(
                    v_flex()
                        .pb_1()
                        .child(ListSeparator)
                        .child(fork_profile)
                        .child(configure_builtin_tools)
                        .child(configure_mcp_tools)
                        .child(delete_profile)
                        .child(ListSeparator)
                        .child(go_back),
                )
                .into_any_element(),
        )
        .entry(mode.fork_profile)
        .entry(mode.configure_tools)
        .entry(mode.configure_mcps)
        .entry(mode.delete_profile)
        .entry(mode.cancel_item)
    }
}

impl Render for ManageProfilesModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let go_back_item = self.render_go_back_row(&self.focus_handle, window, cx);

        div()
            .elevation_3(cx)
            .w(rems(34.))
            .key_context("ManageProfilesModal")
            .on_action(cx.listener(|this, _: &menu::Cancel, window, cx| this.cancel(window, cx)))
            .on_action(cx.listener(|this, _: &menu::Confirm, window, cx| this.confirm(window, cx)))
            .capture_any_mouse_down(cx.listener(|this, _, window, cx| {
                this.focus_handle(cx).focus(window, cx);
            }))
            .on_mouse_down_out(cx.listener(|_this, _, _, cx| cx.emit(DismissEvent)))
            .child(match &self.mode {
                Mode::ChooseProfile(mode) => self
                    .render_choose_profile(mode.clone(), window, cx)
                    .into_any_element(),
                Mode::NewProfile(mode) => self
                    .render_new_profile(mode.clone(), window, cx)
                    .into_any_element(),
                Mode::ViewProfile(mode) => self
                    .render_view_profile(mode.clone(), window, cx)
                    .into_any_element(),
                Mode::ConfigureTools {
                    profile_id,
                    tool_picker,
                    ..
                } => self.render_tool_picker_mode(
                    profile_id,
                    tool_picker,
                    "agent_ui.manage_profiles.header.configure_built_in_tools",
                    "{} — Configure Built-in Tools",
                    IconName::Settings,
                    go_back_item,
                    cx,
                ),
                Mode::ConfigureMcps {
                    profile_id,
                    tool_picker,
                    ..
                } => self.render_tool_picker_mode(
                    profile_id,
                    tool_picker,
                    "agent_ui.manage_profiles.header.configure_mcp_tools",
                    "{} — Configure MCP Tools",
                    IconName::ToolHammer,
                    go_back_item,
                    cx,
                ),
            })
    }
}
