use std::{rc::Rc, sync::Arc};

use acp_thread::AgentSessionConfigOptions;
use agent_client_protocol::v1::schema as acp;
use agent_servers::AgentServer;

use collections::HashSet;
use feature_flags::{AcpBetaFeatureFlag, FeatureFlagAppExt as _};
use fs::Fs;
use fuzzy_nucleo::StringMatchCandidate;
use gpui::{
    App, BackgroundExecutor, Context, DismissEvent, Entity, Focusable as _, Subscription, Task,
    Window, prelude::*,
};
use menu::SelectParent;
use picker::popover_menu::PickerPopoverMenu;
use picker::{Picker, PickerDelegate};
use settings::{AgentConfigOptionValue, SettingsStore};
use ui::{
    ContextMenu, ContextMenuEntry, ElevationIndex, IconButton, KeyBinding, ListItem,
    ListItemSpacing, PopoverMenu, PopoverMenuHandle, Tooltip, prelude::*,
};
use util::ResultExt as _;
use workspace::Workspace;
use zzz_actions::agent::ToggleModelSelector;

use crate::model_search;
use crate::tr;
use crate::ui::{
    HoldForDefault, documentation_aside_side, fast_mode_button, thinking_effort_trigger,
};
use crate::{
    CycleFavoriteModels, CycleModeSelector, CycleThinkingEffort, ToggleFastMode,
    ToggleProfileSelector, ToggleThinkingEffortMenu,
};

fn separator_label(title: &SharedString, cx: &App) -> SharedString {
    match title.as_ref() {
        "agent_ui.config_options.favorites" => {
            tr(cx, "agent_ui.config_options.favorites", "Favorites")
        }
        "agent_ui.config_options.all_options" => {
            tr(cx, "agent_ui.config_options.all_options", "All Options")
        }
        _ => title.clone(),
    }
}

const PICKER_THRESHOLD: usize = 5;

fn toolbar_category(
    option: &acp::SessionConfigOption,
) -> Option<&acp::SessionConfigOptionCategory> {
    use acp::SessionConfigOptionCategory::{Mode, Model, Speed, ThoughtLevel};

    let category = option.category.as_ref()?;
    match (&option.kind, category) {
        (acp::SessionConfigKind::Select(_), Model | ThoughtLevel | Mode)
            if count_config_options(option) > 0 =>
        {
            Some(category)
        }
        (acp::SessionConfigKind::Boolean(_), Speed) => Some(category),
        _ => None,
    }
}

fn split_config_options(
    options: Vec<acp::SessionConfigOption>,
    render_boolean_config_options: bool,
) -> (Vec<acp::SessionConfigOption>, Vec<acp::SessionConfigOption>) {
    use acp::SessionConfigOptionCategory::{Mode, Model, Speed, ThoughtLevel};

    let mut options: Vec<_> = options
        .into_iter()
        .filter(|option| match &option.kind {
            acp::SessionConfigKind::Select(_) => count_config_options(option) > 0,
            acp::SessionConfigKind::Boolean(_) => render_boolean_config_options,
            _ => false,
        })
        .collect();

    let toolbar = [Speed, ThoughtLevel, Mode, Model]
        .iter()
        .filter_map(|category| {
            let index = options
                .iter()
                .position(|option| toolbar_category(option) == Some(category))?;
            Some(options.remove(index))
        })
        .collect();
    (toolbar, options)
}

pub struct ConfigOptionsView {
    config_options: Rc<dyn AgentSessionConfigOptions>,
    selectors: Vec<Entity<ConfigOptionSelector>>,
    has_overflow: bool,
    overflow_menu: PopoverMenuHandle<ContextMenu>,
    agent_server: Rc<dyn AgentServer>,
    fs: Arc<dyn Fs>,
    _refresh_task: Task<()>,
}

impl ConfigOptionsView {
    pub fn new(
        config_options: Rc<dyn AgentSessionConfigOptions>,
        agent_server: Rc<dyn AgentServer>,
        fs: Arc<dyn Fs>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (selectors, has_overflow) =
            Self::build_selectors(&config_options, &agent_server, &fs, window, cx);

        let rx = config_options.watch(cx);
        let refresh_task = cx.spawn_in(window, async move |this, cx| {
            if let Some(mut rx) = rx {
                while rx.recv().await == Ok(()) {
                    this.update_in(cx, |this, window, cx| {
                        this.rebuild_selectors(window, cx);
                    })
                    .log_err();
                }
            }
        });

        Self {
            config_options,
            selectors,
            has_overflow,
            overflow_menu: PopoverMenuHandle::default(),
            agent_server,
            fs,
            _refresh_task: refresh_task,
        }
    }

    pub(crate) fn response_controls<'a>(
        &'a self,
        cx: &'a App,
    ) -> impl Iterator<Item = AnyElement> + 'a {
        self.selectors
            .iter()
            .filter(|selector| selector.read(cx).is_response_control)
            .map(|selector| selector.clone().into_any_element())
    }

    pub fn toggle_category_picker(
        &mut self,
        category: acp::SessionConfigOptionCategory,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(config_id) = self
            .config_options
            .config_options()
            .into_iter()
            .find(|option| {
                toolbar_category(option) == Some(&category)
                    && matches!(&option.kind, acp::SessionConfigKind::Select(_))
            })
            .map(|option| option.id)
        else {
            return false;
        };

        let Some(selector) = self.selector_for_config_id(&config_id, cx) else {
            return false;
        };

        selector.update(cx, |selector, cx| selector.toggle_picker(window, cx))
    }

    pub fn toggle_fast_mode(&self, window: &Window, cx: &mut App) {
        if !should_render_boolean_config_options(cx) {
            return;
        }

        let Some(option) = self
            .config_options
            .config_options()
            .into_iter()
            .find(|option| {
                toolbar_category(option) == Some(&acp::SessionConfigOptionCategory::Speed)
            })
        else {
            return;
        };

        toggle_boolean_config_option(
            self.config_options.clone(),
            self.agent_server.clone(),
            option.id,
            self.fs.clone(),
            window,
            cx,
        );
    }

    pub fn cycle_category_option(
        &mut self,
        category: acp::SessionConfigOptionCategory,
        favorites_only: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let render_boolean_config_options = should_render_boolean_config_options(cx);
        let Some(config_id) = self
            .config_options
            .config_options()
            .into_iter()
            .filter(|option| {
                option.category.as_ref() == Some(&category)
                    && match &option.kind {
                        acp::SessionConfigKind::Select(_) => count_config_options(option) > 0,
                        acp::SessionConfigKind::Boolean(_) => {
                            !favorites_only && render_boolean_config_options
                        }
                        _ => false,
                    }
            })
            .min_by_key(|option| toolbar_category(option).is_none())
            .map(|option| option.id)
        else {
            return false;
        };

        let Some(next_value) = self.next_value_for_config(&config_id, favorites_only, cx) else {
            return false;
        };

        set_config_option_and_default(
            self.config_options.clone(),
            self.agent_server.clone(),
            config_id,
            next_value,
            self.fs.clone(),
            window,
            cx,
        )
        .detach_and_log_err(cx);

        true
    }

    fn selector_for_config_id(
        &self,
        config_id: &acp::SessionConfigId,
        cx: &App,
    ) -> Option<Entity<ConfigOptionSelector>> {
        self.selectors
            .iter()
            .find(|selector| selector.read(cx).config_id() == config_id)
            .cloned()
    }

    fn next_value_for_config(
        &self,
        config_id: &acp::SessionConfigId,
        favorites_only: bool,
        cx: &mut Context<Self>,
    ) -> Option<acp::SessionConfigOptionValue> {
        let option = self
            .config_options
            .config_options()
            .into_iter()
            .find(|option| &option.id == config_id)?;

        match &option.kind {
            acp::SessionConfigKind::Select(_) => {
                let mut options = extract_options(&self.config_options, config_id);
                if options.is_empty() {
                    return None;
                }

                if favorites_only {
                    let favorites = self
                        .agent_server
                        .favorite_config_option_value_ids(config_id, cx);
                    options.retain(|option| favorites.contains(&option.value));
                    if options.is_empty() {
                        return None;
                    }
                }

                let current_value = get_current_select_value(&self.config_options, config_id);
                let next_index = current_value
                    .as_ref()
                    .and_then(|current| options.iter().position(|option| &option.value == current))
                    .map_or(0, |current_index| (current_index + 1) % options.len());

                Some(acp::SessionConfigOptionValue::value_id(
                    options.get(next_index)?.value.clone(),
                ))
            }
            acp::SessionConfigKind::Boolean(boolean) => {
                if favorites_only || !should_render_boolean_config_options(cx) {
                    None
                } else {
                    Some(acp::SessionConfigOptionValue::boolean(
                        !boolean.current_value,
                    ))
                }
            }
            _ => None,
        }
    }

    fn rebuild_selectors(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Config option updates can mutate option values for existing IDs (for example,
        // reasoning levels after a model switch). Rebuild to refresh cached picker entries.
        self.overflow_menu.hide(cx);
        for selector in &self.selectors {
            if let Some(handle) = &selector.read(cx).picker_handle {
                handle.clone().hide(cx);
            }
        }
        (self.selectors, self.has_overflow) = Self::build_selectors(
            &self.config_options,
            &self.agent_server,
            &self.fs,
            window,
            cx,
        );
        cx.notify();
    }

    fn build_selectors(
        config_options: &Rc<dyn AgentSessionConfigOptions>,
        agent_server: &Rc<dyn AgentServer>,
        fs: &Arc<dyn Fs>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Vec<Entity<ConfigOptionSelector>>, bool) {
        let (toolbar, overflow) = split_config_options(
            config_options.config_options(),
            should_render_boolean_config_options(cx),
        );
        let selectors = toolbar
            .into_iter()
            .map(|option| {
                let config_options = config_options.clone();
                let agent_server = agent_server.clone();
                let fs = fs.clone();
                cx.new(|cx| {
                    ConfigOptionSelector::new(config_options, option, agent_server, fs, window, cx)
                })
            })
            .collect();
        (selectors, !overflow.is_empty())
    }

    fn build_overflow_menu(
        &self,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Entity<ContextMenu>> {
        let (_, overflow) = split_config_options(
            self.config_options.config_options(),
            should_render_boolean_config_options(cx),
        );
        if overflow.is_empty() {
            return None;
        }

        let agent_options_title = tr(cx, "agent_ui.config_options.agent_options", "Agent Options");
        Some(ContextMenu::build(window, cx, |mut menu, _, cx| {
            menu = menu.header(agent_options_title);
            for option in overflow {
                let config_options = self.config_options.clone();
                let agent_server = self.agent_server.clone();
                let fs = self.fs.clone();
                let config_id = option.id.clone();

                match &option.kind {
                    acp::SessionConfigKind::Boolean(boolean) => {
                        let entry = ContextMenuEntry::new(option.name)
                            .toggleable(IconPosition::End, boolean.current_value)
                            .handler(move |window, cx| {
                                toggle_boolean_config_option(
                                    config_options.clone(),
                                    agent_server.clone(),
                                    config_id.clone(),
                                    fs.clone(),
                                    window,
                                    cx,
                                );
                            })
                            .when_some(option.description, |entry, description| {
                                entry.documentation_aside(documentation_aside_side(cx), move |_| {
                                    Label::new(description.clone()).into_any_element()
                                })
                            });
                        menu = menu.item(entry);
                    }
                    acp::SessionConfigKind::Select(select) => {
                        let label = find_option_name(&select.options, &select.current_value)
                            .map_or_else(
                                || option.name.clone(),
                                |value| format!("{}: {}", option.name, truncate_value_name(&value)),
                            );
                        menu = menu.submenu(label, move |menu, window, cx| {
                            let Some(option) =
                                config_options.config_options().into_iter().find(|option| {
                                    option.id == config_id && count_config_options(option) > 0
                                })
                            else {
                                return menu;
                            };
                            let picker = build_config_option_picker(
                                config_options.clone(),
                                &option,
                                agent_server.clone(),
                                fs.clone(),
                                true,
                                window,
                                cx,
                            );
                            cx.subscribe_in(
                                &picker,
                                window,
                                |menu, _, _: &DismissEvent, window, cx| {
                                    menu.select_submenu_parent(&SelectParent, window, cx);
                                },
                            )
                            .detach();
                            let focus_handle = picker.focus_handle(cx);
                            menu.embedded_view(picker).focus_content(focus_handle)
                        });
                    }
                    _ => {}
                }
            }
            menu
        }))
    }

    fn render_overflow_menu(&self, cx: &Context<Self>) -> PopoverMenu<ContextMenu> {
        let view = cx.weak_entity();
        let title = tr(cx, "agent_ui.config_options.agent_options", "Agent Options");
        PopoverMenu::new("agent-options-menu")
            .trigger_with_tooltip(
                IconButton::new("agent-options", IconName::Filter)
                    .icon_size(IconSize::Small)
                    .icon_color(Color::Muted)
                    .selected_style(ButtonStyle::Tinted(ui::TintColor::Accent)),
                Tooltip::text(title),
            )
            .anchor(gpui::Anchor::BottomRight)
            .offset(gpui::point(px(0.), px(-2.)))
            .with_handle(self.overflow_menu.clone())
            .menu(move |window, cx| {
                view.upgrade()?
                    .update(cx, |view, cx| view.build_overflow_menu(window, cx))
            })
    }
}

impl Render for ConfigOptionsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.selectors.is_empty() && !self.has_overflow {
            return div().into_any_element();
        }

        h_flex()
            .min_w_0()
            .flex_wrap()
            .gap_1()
            .when(self.has_overflow, |this| {
                this.child(self.render_overflow_menu(cx))
            })
            .children(
                self.selectors
                    .iter()
                    .filter(|selector| !selector.read(cx).is_response_control)
                    .cloned(),
            )
            .into_any_element()
    }
}

struct ConfigOptionSelector {
    config_options: Rc<dyn AgentSessionConfigOptions>,
    config_id: acp::SessionConfigId,
    is_response_control: bool,
    agent_server: Rc<dyn AgentServer>,
    fs: Arc<dyn Fs>,
    picker_handle: Option<PopoverMenuHandle<Picker<ConfigOptionPickerDelegate>>>,
    picker: Option<Entity<Picker<ConfigOptionPickerDelegate>>>,
}

impl ConfigOptionSelector {
    const TRUNCATED_LABEL_LIMIT: usize = 32;

    fn new(
        config_options: Rc<dyn AgentSessionConfigOptions>,
        option: acp::SessionConfigOption,
        agent_server: Rc<dyn AgentServer>,
        fs: Arc<dyn Fs>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let is_response_control = matches!(
            toolbar_category(&option),
            Some(
                acp::SessionConfigOptionCategory::Speed
                    | acp::SessionConfigOptionCategory::ThoughtLevel
            )
        );
        let (picker_handle, picker) = if matches!(&option.kind, acp::SessionConfigKind::Select(_)) {
            let picker = build_config_option_picker(
                config_options.clone(),
                &option,
                agent_server.clone(),
                fs.clone(),
                false,
                window,
                cx,
            );
            (Some(PopoverMenuHandle::default()), Some(picker))
        } else {
            (None, None)
        };

        Self {
            config_options,
            config_id: option.id,
            is_response_control,
            agent_server,
            fs,
            picker_handle,
            picker,
        }
    }

    fn current_option(&self) -> Option<acp::SessionConfigOption> {
        self.config_options
            .config_options()
            .into_iter()
            .find(|opt| opt.id == self.config_id)
    }

    fn config_id(&self) -> &acp::SessionConfigId {
        &self.config_id
    }

    fn toggle_picker(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !config_option_is_select(&self.config_options, &self.config_id) {
            return false;
        }

        if let Some(picker_handle) = &self.picker_handle {
            picker_handle.toggle(window, cx);
            true
        } else {
            false
        }
    }
}

impl Render for ConfigOptionSelector {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(option) = self.current_option() else {
            return div().into_any_element();
        };

        match &option.kind {
            acp::SessionConfigKind::Select(select) => {
                let (Some(picker), Some(picker_handle)) =
                    (self.picker.clone(), self.picker_handle.clone())
                else {
                    return div().into_any_element();
                };

                let id = ElementId::Name(format!("config-option-{}", option.id.0).into());
                let value_name = find_option_name(&select.options, &select.current_value);
                let value_name = value_name.as_deref().unwrap_or(&option.name);
                let label = truncate_button_label(value_name, Self::TRUNCATED_LABEL_LIMIT);
                let menu_open = picker_handle.is_deployed();

                let option_category = option.category.clone();
                let option_name = option.name;
                let option_description: Option<SharedString> = option.description.map(Into::into);

                let tooltip = Tooltip::element(move |_window, cx| {
                    let mut content = v_flex().gap_1().child(Label::new(option_name.clone()));
                    if let Some(desc) = option_description.as_ref() {
                        content = content.child(
                            Label::new(desc.clone())
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        );
                    }

                    let action_tooltip_container = |label: SharedString, keybinding: KeyBinding| {
                        h_flex()
                            .pt_1()
                            .gap_2()
                            .justify_between()
                            .border_t_1()
                            .border_color(cx.theme().colors().border_variant)
                            .child(Label::new(label))
                            .child(keybinding)
                    };

                    if let Some(category) = &option_category {
                        match category {
                            acp::SessionConfigOptionCategory::Mode => {
                                content = content
                                    .child(action_tooltip_container(
                                        tr(cx, "agent_ui.mode_selector.change_mode", "Change Mode"),
                                        KeyBinding::for_action(&ToggleProfileSelector, cx),
                                    ))
                                    .child(action_tooltip_container(
                                        tr(
                                            cx,
                                            "agent_ui.mode_selector.cycle_through_modes",
                                            "Cycle Through Modes",
                                        ),
                                        KeyBinding::for_action(&CycleModeSelector, cx),
                                    ));
                            }
                            acp::SessionConfigOptionCategory::Model => {
                                content = content
                                    .child(action_tooltip_container(
                                        tr(
                                            cx,
                                            "agent_ui.model_selector.change_model",
                                            "Change Model",
                                        ),
                                        KeyBinding::for_action(&ToggleModelSelector, cx),
                                    ))
                                    .child(action_tooltip_container(
                                        tr(
                                            cx,
                                            "agent_ui.model_selector.cycle_favorite_models",
                                            "Cycle Favorite Models",
                                        ),
                                        KeyBinding::for_action(&CycleFavoriteModels, cx),
                                    ));
                            }
                            acp::SessionConfigOptionCategory::ThoughtLevel => {
                                content = content
                                    .child(action_tooltip_container(
                                        tr(
                                            cx,
                                            "agent_ui.config_options.change_thinking_effort",
                                            "Change Thinking Effort",
                                        ),
                                        KeyBinding::for_action(&ToggleThinkingEffortMenu, cx),
                                    ))
                                    .child(action_tooltip_container(
                                        tr(
                                            cx,
                                            "agent_ui.config_options.cycle_thinking_effort",
                                            "Cycle Thinking Effort",
                                        ),
                                        KeyBinding::for_action(&CycleThinkingEffort, cx),
                                    ));
                            }
                            _ => {}
                        }
                    }
                    content.into_any()
                });

                if self.is_response_control {
                    return PickerPopoverMenu::new(
                        picker,
                        thinking_effort_trigger(id, label, menu_open, true),
                        tooltip,
                        gpui::Anchor::BottomLeft,
                        cx,
                    )
                    .with_handle(picker_handle)
                    .render(window, cx)
                    .into_any_element();
                }

                let (chevron, color) = if menu_open {
                    (IconName::ChevronUp, Color::Accent)
                } else {
                    (IconName::ChevronDown, Color::Muted)
                };
                let trigger_button = Button::new(id, label)
                    .label_size(LabelSize::Small)
                    .color(color)
                    .end_icon(Icon::new(chevron).size(IconSize::XSmall).color(color))
                    .selected_style(ButtonStyle::Tinted(ui::TintColor::Accent));

                PickerPopoverMenu::new(
                    picker,
                    trigger_button,
                    tooltip,
                    gpui::Anchor::BottomRight,
                    cx,
                )
                .with_handle(picker_handle)
                .render(window, cx)
                .into_any_element()
            }
            acp::SessionConfigKind::Boolean(boolean) => {
                let action = if boolean.current_value {
                    tr(
                        cx,
                        "agent_ui.config_options.disable_option",
                        "Disable {name}",
                    )
                } else {
                    tr(cx, "agent_ui.config_options.enable_option", "Enable {name}")
                };
                let tooltip_label = action.replace("{name}", &option.name);
                let option_description: Option<SharedString> = option.description.map(Into::into);

                fast_mode_button(
                    ElementId::Name(format!("config-option-{}", option.id.0).into()),
                    boolean.current_value,
                    cx,
                )
                .tooltip(Tooltip::element(move |_window, cx| {
                    v_flex()
                        .gap_1()
                        .child(
                            h_flex()
                                .gap_2()
                                .justify_between()
                                .child(Label::new(tooltip_label.clone()))
                                .child(KeyBinding::for_action(&ToggleFastMode, cx)),
                        )
                        .when_some(option_description.as_ref(), |this, description| {
                            this.child(
                                Label::new(description.clone())
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            )
                        })
                        .into_any()
                }))
                .on_click(cx.listener(|this, _, window, cx| {
                    toggle_boolean_config_option(
                        this.config_options.clone(),
                        this.agent_server.clone(),
                        this.config_id.clone(),
                        this.fs.clone(),
                        window,
                        cx,
                    );
                }))
                .into_any_element()
            }
            _ => div().into_any_element(),
        }
    }
}

fn build_config_option_picker(
    config_options: Rc<dyn AgentSessionConfigOptions>,
    option: &acp::SessionConfigOption,
    agent_server: Rc<dyn AgentServer>,
    fs: Arc<dyn Fs>,
    embedded: bool,
    window: &mut Window,
    cx: &mut App,
) -> Entity<Picker<ConfigOptionPickerDelegate>> {
    let searchable = count_config_options(option) >= PICKER_THRESHOLD;
    cx.new(|cx| {
        let mut delegate = ConfigOptionPickerDelegate::new(
            config_options,
            option.id.clone(),
            agent_server,
            fs,
            window,
            cx,
        );
        // Overflow pickers have no toolbar trigger to show the option's description.
        if embedded {
            delegate.option_description = option.description.clone().map(Into::into);
        }
        let picker = if searchable {
            Picker::list(delegate, window, cx)
        } else {
            Picker::nonsearchable_list(delegate, window, cx)
        }
        .show_scrollbar(true)
        .width(rems(20.))
        .max_height(Some(rems(20.).into()));
        if embedded {
            picker.embedded().reopenable(false, cx)
        } else {
            picker
        }
    })
}

#[derive(Clone)]
enum ConfigOptionPickerEntry {
    Separator(SharedString),
    Option(ConfigOptionValue),
}

#[derive(Clone)]
struct ConfigOptionValue {
    value: acp::SessionConfigValueId,
    name: String,
    description: Option<String>,
    group: Option<String>,
}

struct ConfigOptionPickerDelegate {
    config_options: Rc<dyn AgentSessionConfigOptions>,
    config_id: acp::SessionConfigId,
    agent_server: Rc<dyn AgentServer>,
    fs: Arc<dyn Fs>,
    filtered_entries: Vec<ConfigOptionPickerEntry>,
    all_options: Vec<ConfigOptionValue>,
    option_description: Option<SharedString>,
    selected_index: usize,
    selected_description: Option<(usize, SharedString, bool)>,
    favorites: HashSet<acp::SessionConfigValueId>,
    _settings_subscription: Subscription,
}

impl ConfigOptionPickerDelegate {
    fn new(
        config_options: Rc<dyn AgentSessionConfigOptions>,
        config_id: acp::SessionConfigId,
        agent_server: Rc<dyn AgentServer>,
        fs: Arc<dyn Fs>,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Self {
        let favorites = agent_server.favorite_config_option_value_ids(&config_id, cx);

        let all_options = extract_options(&config_options, &config_id);
        let filtered_entries = options_to_picker_entries(&all_options, &favorites);

        let current_value = get_current_select_value(&config_options, &config_id);
        let selected_index = current_value
            .and_then(|current| {
                filtered_entries.iter().position(|entry| {
                    matches!(entry, ConfigOptionPickerEntry::Option(opt) if opt.value == current)
                })
            })
            .unwrap_or(0);

        let agent_server_for_subscription = agent_server.clone();
        let config_id_for_subscription = config_id.clone();
        let settings_subscription =
            cx.observe_global_in::<SettingsStore>(window, move |picker, window, cx| {
                let new_favorites = agent_server_for_subscription
                    .favorite_config_option_value_ids(&config_id_for_subscription, cx);
                if new_favorites != picker.delegate.favorites {
                    picker.delegate.favorites = new_favorites;
                    picker.refresh(window, cx);
                }
            });

        cx.notify();

        Self {
            config_options,
            config_id,
            agent_server,
            fs,
            filtered_entries,
            all_options,
            option_description: None,
            selected_index,
            selected_description: None,
            favorites,
            _settings_subscription: settings_subscription,
        }
    }

    fn current_value(&self) -> Option<acp::SessionConfigValueId> {
        get_current_select_value(&self.config_options, &self.config_id)
    }
}

impl PickerDelegate for ConfigOptionPickerDelegate {
    type ListItem = AnyElement;

    fn match_count(&self) -> usize {
        self.filtered_entries.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(&mut self, ix: usize, _: &mut Window, cx: &mut Context<Picker<Self>>) {
        self.selected_index = ix.min(self.filtered_entries.len().saturating_sub(1));
        cx.notify();
    }

    fn can_select(&self, ix: usize, _window: &mut Window, _cx: &mut Context<Picker<Self>>) -> bool {
        match self.filtered_entries.get(ix) {
            Some(ConfigOptionPickerEntry::Option(option)) => {
                config_option_has_value(&self.config_options, &self.config_id, &option.value)
            }
            Some(ConfigOptionPickerEntry::Separator(_)) | None => false,
        }
    }

    fn placeholder_text(&self, _window: &mut Window, cx: &mut App) -> Arc<str> {
        Arc::from(tr(
            cx,
            "agent_ui.config_options.select_an_option",
            "Select an option...",
        ))
    }

    fn render_header(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) -> Option<AnyElement> {
        let description = self.option_description.clone()?;
        Some(
            div()
                .id("config-option-description")
                .debug_selector(|| "config-option-description".into())
                .px_2()
                .py_1()
                .tooltip(Tooltip::text(description.clone()))
                .child(
                    Label::new(description)
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .line_clamp(3),
                )
                .into_any_element(),
        )
    }

    fn update_matches(
        &mut self,
        query: String,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let all_options = self.all_options.clone();
        let background_executor = cx.background_executor().clone();

        cx.spawn_in(window, async move |this, cx| {
            let filtered_options = if query.is_empty() {
                all_options
            } else {
                fuzzy_search_options(all_options, &query, background_executor).await
            };
            let Some(this) = this.upgrade() else {
                return;
            };

            this.update_in(cx, |this, window, cx| {
                this.delegate.filtered_entries =
                    options_to_picker_entries(&filtered_options, &this.delegate.favorites);

                let current_value = this.delegate.current_value();
                let new_index = current_value
                    .and_then(|current| {
                        this.delegate.filtered_entries.iter().position(|entry| {
                            matches!(entry, ConfigOptionPickerEntry::Option(opt) if opt.value == current)
                        })
                    })
                    .unwrap_or(0);

                this.set_selected_index(new_index, Some(picker::Direction::Down), true, window, cx);
                cx.notify();
            })
            .log_err();
        })
    }

    fn confirm(&mut self, _secondary: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        if let Some(ConfigOptionPickerEntry::Option(option)) =
            self.filtered_entries.get(self.selected_index)
        {
            if !config_option_has_value(&self.config_options, &self.config_id, &option.value) {
                return;
            }

            if window.modifiers().secondary() {
                let default_value = self
                    .agent_server
                    .default_config_option(self.config_id.0.as_ref(), cx);
                let is_default = default_value
                    .as_ref()
                    .and_then(AgentConfigOptionValue::as_value_id)
                    == Some(option.value.0.as_ref());

                self.agent_server.set_default_config_option(
                    self.config_id.0.as_ref(),
                    (!is_default)
                        .then(|| AgentConfigOptionValue::ValueId(option.value.0.to_string())),
                    self.fs.clone(),
                    cx,
                );
            }

            if window.modifiers().secondary() {
                let task = self.config_options.set_config_option(
                    self.config_id.clone(),
                    acp::SessionConfigOptionValue::value_id(option.value.clone()),
                    cx,
                );

                cx.spawn(async move |_, _| {
                    if let Err(err) = task.await {
                        log::error!("Failed to set config option: {:?}", err);
                    }
                })
                .detach();
            } else {
                set_config_option_and_default(
                    self.config_options.clone(),
                    self.agent_server.clone(),
                    self.config_id.clone(),
                    acp::SessionConfigOptionValue::value_id(option.value.clone()),
                    self.fs.clone(),
                    window,
                    cx,
                )
                .detach_and_log_err(cx);
            }

            cx.emit(DismissEvent);
        }
    }

    fn dismissed(&mut self, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        cx.defer_in(window, |picker, window, cx| {
            picker.set_query("", window, cx);
        });
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        _: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        match self.filtered_entries.get(ix)? {
            ConfigOptionPickerEntry::Separator(title) => Some(
                div()
                    .when(ix > 0, |this| this.mt_1())
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .text_xs()
                            .text_color(cx.theme().colors().text_muted)
                            .child(separator_label(title, cx)),
                    )
                    .into_any_element(),
            ),
            ConfigOptionPickerEntry::Option(option) => {
                let current_value = self.current_value();
                let is_selected = current_value.as_ref() == Some(&option.value);

                let default_value = self
                    .agent_server
                    .default_config_option(self.config_id.0.as_ref(), cx);
                let is_default = default_value
                    .as_ref()
                    .and_then(AgentConfigOptionValue::as_value_id)
                    == Some(option.value.0.as_ref());

                let is_favorite = self.favorites.contains(&option.value);

                let option_name = option.name.clone();
                let description = option.description.clone();

                Some(
                    div()
                        .id(("config-option-picker-item", ix))
                        .when_some(description, |this, desc| {
                            let desc: SharedString = desc.into();
                            this.on_hover(cx.listener(move |menu, hovered, _, cx| {
                                if *hovered {
                                    menu.delegate.selected_description =
                                        Some((ix, desc.clone(), is_default));
                                } else if matches!(menu.delegate.selected_description, Some((id, _, _)) if id == ix)
                                {
                                    menu.delegate.selected_description = None;
                                }
                                cx.notify();
                            }))
                        })
                        .child(
                            ListItem::new(ix)
                                .inset(true)
                                .spacing(ListItemSpacing::Sparse)
                                .toggle_state(selected)
                                .child(h_flex().w_full().child(Label::new(option_name).truncate()))
                                .end_slot(div().pr_2().when(is_selected, |this| {
                                    this.child(Icon::new(IconName::Check).color(Color::Accent))
                                }))
                                .end_slot_on_hover(div().pr_1p5().child({
                                    let (icon, color, tooltip) = if is_favorite {
                                        (
                                            IconName::StarFilled,
                                            Color::Accent,
                                            tr(
                                                cx,
                                                "agent_ui.config_options.unfavorite",
                                                "Unfavorite",
                                            ),
                                        )
                                    } else {
                                        (
                                            IconName::Star,
                                            Color::Default,
                                            tr(
                                                cx,
                                                "agent_ui.config_options.favorite",
                                                "Favorite",
                                            ),
                                        )
                                    };

                                    let config_id = self.config_id.clone();
                                    let value_id = option.value.clone();
                                    let agent_server = self.agent_server.clone();
                                    let fs = self.fs.clone();
                                    let config_options = self.config_options.clone();

                                    IconButton::new(("toggle-favorite-config-option", ix), icon)
                                        .layer(ElevationIndex::ElevatedSurface)
                                        .icon_color(color)
                                        .icon_size(IconSize::Small)
                                        .tooltip(Tooltip::text(tooltip))
                                        .on_click(move |_, _, cx| {
                                            if !config_option_has_value(
                                                &config_options,
                                                &config_id,
                                                &value_id,
                                            ) {
                                                return;
                                            }

                                            agent_server.toggle_favorite_config_option_value(
                                                config_id.clone(),
                                                value_id.clone(),
                                                !is_favorite,
                                                fs.clone(),
                                                cx,
                                            );
                                        })
                                })),
                        )
                        .into_any_element(),
                )
            }
        }
    }

    fn documentation_aside(
        &self,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<ui::DocumentationAside> {
        self.selected_description
            .as_ref()
            .map(|(_, description, is_default)| {
                let description = description.clone();
                let is_default = *is_default;

                let side = documentation_aside_side(cx);

                ui::DocumentationAside::new(
                    side,
                    Rc::new(move |_| {
                        v_flex()
                            .gap_1()
                            .child(Label::new(description.clone()))
                            .child(HoldForDefault::new(is_default))
                            .into_any_element()
                    }),
                )
            })
    }

    fn documentation_aside_index(&self) -> Option<usize> {
        self.selected_description.as_ref().map(|(ix, _, _)| *ix)
    }
}

fn extract_options(
    config_options: &Rc<dyn AgentSessionConfigOptions>,
    config_id: &acp::SessionConfigId,
) -> Vec<ConfigOptionValue> {
    let Some(option) = config_options
        .config_options()
        .into_iter()
        .find(|opt| &opt.id == config_id)
    else {
        return Vec::new();
    };

    match &option.kind {
        acp::SessionConfigKind::Select(select) => match &select.options {
            acp::SessionConfigSelectOptions::Ungrouped(options) => options
                .iter()
                .map(|opt| ConfigOptionValue {
                    value: opt.value.clone(),
                    name: opt.name.clone(),
                    description: opt.description.clone(),
                    group: None,
                })
                .collect(),
            acp::SessionConfigSelectOptions::Grouped(groups) => groups
                .iter()
                .flat_map(|group| {
                    group.options.iter().map(|opt| ConfigOptionValue {
                        value: opt.value.clone(),
                        name: opt.name.clone(),
                        description: opt.description.clone(),
                        group: Some(group.name.clone()),
                    })
                })
                .collect(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

fn config_option_is_select(
    config_options: &Rc<dyn AgentSessionConfigOptions>,
    config_id: &acp::SessionConfigId,
) -> bool {
    config_options.config_options().into_iter().any(|option| {
        &option.id == config_id && matches!(option.kind, acp::SessionConfigKind::Select(_))
    })
}

fn config_option_has_value(
    config_options: &Rc<dyn AgentSessionConfigOptions>,
    config_id: &acp::SessionConfigId,
    value_id: &acp::SessionConfigValueId,
) -> bool {
    extract_options(config_options, config_id)
        .iter()
        .any(|option| &option.value == value_id)
}

fn get_current_select_value(
    config_options: &Rc<dyn AgentSessionConfigOptions>,
    config_id: &acp::SessionConfigId,
) -> Option<acp::SessionConfigValueId> {
    config_options
        .config_options()
        .into_iter()
        .find(|opt| &opt.id == config_id)
        .and_then(|opt| match &opt.kind {
            acp::SessionConfigKind::Select(select) => Some(select.current_value.clone()),
            _ => None,
        })
}

fn set_config_option_and_default(
    config_options: Rc<dyn AgentSessionConfigOptions>,
    agent_server: Rc<dyn AgentServer>,
    config_id: acp::SessionConfigId,
    value: acp::SessionConfigOptionValue,
    fs: Arc<dyn Fs>,
    window: &Window,
    cx: &mut App,
) -> Task<anyhow::Result<()>> {
    let workspace = Workspace::for_window(window, cx).map(|workspace| workspace.downgrade());
    let response = config_options.set_config_option(config_id.clone(), value, cx);
    cx.spawn(async move |cx| {
        let options = match response.await {
            Ok(options) => options,
            Err(error) => {
                if let Some(workspace) = workspace.and_then(|workspace| workspace.upgrade()) {
                    workspace.update(cx, |workspace, cx| {
                        let message = tr(
                            cx,
                            "agent_ui.config_options.change_option_failed",
                            "Could not change agent option",
                        );
                        workspace.show_error(&anyhow::anyhow!("{message}: {error:#}"), cx);
                    });
                }
                return Err(error);
            }
        };
        // Saving the request would make rejected or normalized choices future defaults.
        if let Some(value) = options
            .iter()
            .find(|option| option.id == config_id)
            .and_then(setting_value_for_config_option)
        {
            cx.update(|cx| {
                agent_server.set_default_config_option(config_id.0.as_ref(), Some(value), fs, cx);
            });
        }
        Ok(())
    })
}

fn toggle_boolean_config_option(
    config_options: Rc<dyn AgentSessionConfigOptions>,
    agent_server: Rc<dyn AgentServer>,
    config_id: acp::SessionConfigId,
    fs: Arc<dyn Fs>,
    window: &Window,
    cx: &mut App,
) {
    let Some(acp::SessionConfigKind::Boolean(boolean)) = config_options
        .config_options()
        .into_iter()
        .find(|option| option.id == config_id)
        .map(|option| option.kind)
    else {
        return;
    };
    set_config_option_and_default(
        config_options,
        agent_server,
        config_id,
        acp::SessionConfigOptionValue::boolean(!boolean.current_value),
        fs,
        window,
        cx,
    )
    .detach_and_log_err(cx);
}

fn setting_value_for_config_option(
    option: &acp::SessionConfigOption,
) -> Option<AgentConfigOptionValue> {
    match &option.kind {
        acp::SessionConfigKind::Select(select)
            if find_option_name(&select.options, &select.current_value).is_some() =>
        {
            Some(AgentConfigOptionValue::ValueId(
                select.current_value.0.to_string(),
            ))
        }
        acp::SessionConfigKind::Boolean(boolean) => {
            Some(AgentConfigOptionValue::Boolean(boolean.current_value))
        }
        _ => None,
    }
}

fn should_render_boolean_config_options(cx: &App) -> bool {
    cx.has_flag::<AcpBetaFeatureFlag>()
}

fn options_to_picker_entries(
    options: &[ConfigOptionValue],
    favorites: &HashSet<acp::SessionConfigValueId>,
) -> Vec<ConfigOptionPickerEntry> {
    let mut entries = Vec::new();

    let mut favorite_options = Vec::new();

    for option in options {
        if favorites.contains(&option.value) {
            favorite_options.push(option.clone());
        }
    }

    if !favorite_options.is_empty() {
        entries.push(ConfigOptionPickerEntry::Separator(
            "agent_ui.config_options.favorites".into(),
        ));
        for option in favorite_options {
            entries.push(ConfigOptionPickerEntry::Option(option));
        }

        // If the remaining list would start ungrouped (group == None), insert a separator so
        // Favorites doesn't visually run into the main list.
        if let Some(option) = options.first()
            && option.group.is_none()
        {
            entries.push(ConfigOptionPickerEntry::Separator(
                "agent_ui.config_options.all_options".into(),
            ));
        }
    }

    let mut current_group: Option<String> = None;
    for option in options {
        if option.group != current_group {
            if let Some(group_name) = &option.group {
                entries.push(ConfigOptionPickerEntry::Separator(
                    group_name.clone().into(),
                ));
            }
            current_group = option.group.clone();
        }
        entries.push(ConfigOptionPickerEntry::Option(option.clone()));
    }

    entries
}

async fn fuzzy_search_options(
    options: Vec<ConfigOptionValue>,
    query: &str,
    executor: BackgroundExecutor,
) -> Vec<ConfigOptionValue> {
    model_search::search(options, query, executor, |index, option| {
        (
            StringMatchCandidate::new(index, option.name.clone()),
            StringMatchCandidate::new(
                index,
                format!(
                    "{} {} {} {}",
                    option.name,
                    option.group.as_deref().unwrap_or_default(),
                    option.description.as_deref().unwrap_or_default(),
                    option.value.0,
                ),
            ),
        )
    })
    .await
}

fn find_option_name(
    options: &acp::SessionConfigSelectOptions,
    value_id: &acp::SessionConfigValueId,
) -> Option<String> {
    match options {
        acp::SessionConfigSelectOptions::Ungrouped(opts) => opts
            .iter()
            .find(|o| &o.value == value_id)
            .map(|o| o.name.clone()),
        acp::SessionConfigSelectOptions::Grouped(groups) => groups.iter().find_map(|group| {
            group
                .options
                .iter()
                .find(|o| &o.value == value_id)
                .map(|o| o.name.clone())
        }),
        _ => None,
    }
}

fn count_config_options(option: &acp::SessionConfigOption) -> usize {
    match &option.kind {
        acp::SessionConfigKind::Select(select) => match &select.options {
            acp::SessionConfigSelectOptions::Ungrouped(options) => options.len(),
            acp::SessionConfigSelectOptions::Grouped(groups) => {
                groups.iter().map(|g| g.options.len()).sum()
            }
            _ => 0,
        },
        _ => 0,
    }
}

fn truncate_button_label(label: &str, max_chars: usize) -> String {
    let mut chars = label.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

fn truncate_value_name(value_name: &str) -> String {
    truncate_button_label(value_name, ConfigOptionSelector::TRUNCATED_LABEL_LIMIT)
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::cell::RefCell;

    use acp_thread::AgentConnection;
    use agent_servers::AgentServerDelegate;
    use fs::FakeFs;
    use gpui::TestAppContext;
    use parking_lot::Mutex;
    use project::{AgentId, Project};

    use super::*;

    fn option_names(options: Vec<ConfigOptionValue>) -> Vec<String> {
        options.into_iter().map(|option| option.name).collect()
    }

    #[gpui::test]
    async fn config_option_search_matches_all_searchable_fields(executor: BackgroundExecutor) {
        let options = vec![
            ConfigOptionValue {
                value: acp::SessionConfigValueId::new("openai/gpt-5.6-sol"),
                name: "GPT-5.6 Sol".to_string(),
                description: Some("Flagship reasoning model".to_string()),
                group: Some("OpenAI".to_string()),
            },
            ConfigOptionValue {
                value: acp::SessionConfigValueId::new("anthropic/claude-sonnet-4"),
                name: "Claude Sonnet 4".to_string(),
                description: None,
                group: Some("Anthropic".to_string()),
            },
        ];

        for query in [
            "GPT 5.6",
            "GPT-5.6",
            "gpt5.6 sol",
            "openai",
            "openai/gpt-5.6-sol",
            "flagship",
            "openai reasoning",
        ] {
            assert_eq!(
                option_names(fuzzy_search_options(options.clone(), query, executor.clone()).await),
                ["GPT-5.6 Sol"],
                "query {query:?} should match the GPT model",
            );
        }
    }

    #[gpui::test]
    async fn config_option_search_ranks_names_before_metadata(executor: BackgroundExecutor) {
        let options = vec![
            ConfigOptionValue {
                value: acp::SessionConfigValueId::new("reasoner"),
                name: "Reasoner".to_string(),
                description: Some("High reasoning effort".to_string()),
                group: None,
            },
            ConfigOptionValue {
                value: acp::SessionConfigValueId::new("xhigh"),
                name: "Extra High".to_string(),
                description: None,
                group: None,
            },
            ConfigOptionValue {
                value: acp::SessionConfigValueId::new("high"),
                name: "High".to_string(),
                description: Some(
                    "High effort with advanced reasoning for complex tasks.".to_string(),
                ),
                group: None,
            },
        ];

        assert_eq!(
            option_names(fuzzy_search_options(options, "high", executor).await),
            ["High", "Extra High", "Reasoner"],
        );
    }

    #[gpui::test]
    async fn config_option_search_preserves_name_order_with_different_metadata(
        executor: BackgroundExecutor,
    ) {
        let options = vec![
            ConfigOptionValue {
                value: acp::SessionConfigValueId::new("model-a"),
                name: "Model A".to_string(),
                description: Some("Advanced reasoning for complex tasks".to_string()),
                group: Some("Provider A".to_string()),
            },
            ConfigOptionValue {
                value: acp::SessionConfigValueId::new("b"),
                name: "Model B".to_string(),
                description: None,
                group: None,
            },
        ];

        assert_eq!(
            option_names(fuzzy_search_options(options, "model", executor).await),
            ["Model A", "Model B"],
        );
    }

    #[gpui::test]
    async fn config_option_search_returns_more_than_one_hundred_matches(
        executor: BackgroundExecutor,
    ) {
        let options = (0..101)
            .map(|index| ConfigOptionValue {
                value: acp::SessionConfigValueId::new(format!("model-{index}")),
                name: format!("Model {index}"),
                description: None,
                group: None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            fuzzy_search_options(options, "model", executor).await.len(),
            101
        );
    }

    struct TestConfigOptions {
        options: RefCell<Vec<acp::SessionConfigOption>>,
        set_values: RefCell<Vec<(String, acp::SessionConfigOptionValue)>>,
    }

    impl TestConfigOptions {
        fn new(options: Vec<acp::SessionConfigOption>) -> Self {
            Self {
                options: RefCell::new(options),
                set_values: RefCell::new(Vec::new()),
            }
        }
    }

    impl AgentSessionConfigOptions for TestConfigOptions {
        fn config_options(&self) -> Vec<acp::SessionConfigOption> {
            self.options.borrow().clone()
        }

        fn set_config_option(
            &self,
            config_id: acp::SessionConfigId,
            value: acp::SessionConfigOptionValue,
            _cx: &mut App,
        ) -> Task<anyhow::Result<Vec<acp::SessionConfigOption>>> {
            self.set_values
                .borrow_mut()
                .push((config_id.0.to_string(), value.clone()));

            for option in self.options.borrow_mut().iter_mut() {
                if option.id != config_id {
                    continue;
                }
                match (&mut option.kind, value.clone()) {
                    (
                        acp::SessionConfigKind::Select(select),
                        acp::SessionConfigOptionValue::ValueId { value },
                    ) => select.current_value = value,
                    (
                        acp::SessionConfigKind::Boolean(boolean),
                        acp::SessionConfigOptionValue::Boolean { value },
                    ) => boolean.current_value = value,
                    _ => {}
                }
            }

            Task::ready(Ok(self.config_options()))
        }
    }

    #[test]
    fn composer_uses_the_first_matching_option_for_each_toolbar_control() {
        use acp::SessionConfigOptionCategory::{Mode, Model, Speed, ThoughtLevel};

        let select = |id, category| {
            acp::SessionConfigOption::select(
                id,
                id,
                "value",
                vec![acp::SessionConfigSelectOption::new("value", "Value")],
            )
            .category(category)
        };
        let (toolbar, overflow) = split_config_options(
            vec![
                select("model", Model),
                acp::SessionConfigOption::boolean("extra-mode", "Extra Mode", true).category(Mode),
                select("effort", ThoughtLevel),
                select("another-model", Model),
                select("speed-select", Speed),
                acp::SessionConfigOption::boolean("fast-mode", "Fast Mode", true).category(Speed),
                acp::SessionConfigOption::boolean("tools", "Tools", true),
                acp::SessionConfigOption::select(
                    "empty-mode",
                    "Empty",
                    "none",
                    Vec::<acp::SessionConfigSelectOption>::new(),
                )
                .category(Mode),
                select("mode", Mode),
            ],
            true,
        );
        let ids = |options: Vec<acp::SessionConfigOption>| {
            options
                .into_iter()
                .map(|option| option.id.to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(toolbar), ["fast-mode", "effort", "mode", "model"]);
        assert_eq!(
            ids(overflow),
            ["extra-mode", "another-model", "speed-select", "tools"]
        );
    }

    #[test]
    fn composer_drops_boolean_options_without_the_beta_flag() {
        let (toolbar, overflow) = split_config_options(
            vec![
                acp::SessionConfigOption::boolean("fast-mode", "Fast Mode", true)
                    .category(acp::SessionConfigOptionCategory::Speed),
                acp::SessionConfigOption::boolean("tools", "Tools", true),
            ],
            false,
        );
        assert!(toolbar.is_empty());
        assert!(overflow.is_empty());
    }

    #[gpui::test]
    fn toggling_fast_mode_uses_the_boolean_speed_option(cx: &mut TestAppContext) {
        let agent_server = Rc::new(TestAgentServer::default());
        let config_options = Rc::new(TestConfigOptions::new(vec![
            acp::SessionConfigOption::select(
                "speed",
                "Speed",
                "standard",
                vec![acp::SessionConfigSelectOption::new("standard", "Standard")],
            )
            .category(acp::SessionConfigOptionCategory::Speed),
            acp::SessionConfigOption::boolean("web_search", "Web Search", false)
                .category(acp::SessionConfigOptionCategory::ModelConfig),
            acp::SessionConfigOption::boolean("fast_mode", "Fast Mode", false)
                .category(acp::SessionConfigOptionCategory::Speed),
        ]));
        let fs: Arc<dyn Fs> = FakeFs::new(cx.executor());
        let cx = cx.add_empty_window();

        cx.update(|window, cx| {
            let view = cx.new(|cx| {
                ConfigOptionsView::new(config_options.clone(), agent_server.clone(), fs, window, cx)
            });
            view.update(cx, |view, cx| view.toggle_fast_mode(window, cx));
        });
        cx.run_until_parked();

        assert_eq!(
            agent_server.saved_defaults.lock().as_slice(),
            &[(
                "fast_mode".to_string(),
                Some(AgentConfigOptionValue::Boolean(true))
            )]
        );
        assert_eq!(
            config_options.set_values.borrow().as_slice(),
            &[(
                "fast_mode".to_string(),
                acp::SessionConfigOptionValue::boolean(true)
            )]
        );
    }

    #[test]
    fn setting_value_comes_from_authoritative_option() {
        let select = acp::SessionConfigOption::select(
            "model",
            "Model",
            "auto",
            vec![
                acp::SessionConfigSelectOption::new("auto", "Auto"),
                acp::SessionConfigSelectOption::new("manual", "Manual"),
            ],
        );
        assert_eq!(
            setting_value_for_config_option(&select),
            Some(AgentConfigOptionValue::ValueId("auto".into()))
        );

        let boolean = acp::SessionConfigOption::boolean("enabled", "Enabled", false);
        assert_eq!(
            setting_value_for_config_option(&boolean),
            Some(AgentConfigOptionValue::Boolean(false))
        );

        let stale = acp::SessionConfigOption::select(
            "model",
            "Model",
            "gone",
            vec![acp::SessionConfigSelectOption::new("auto", "Auto")],
        );
        assert_eq!(setting_value_for_config_option(&stale), None);
    }

    #[test]
    fn stale_config_controls_revalidate_current_kind_and_choices() {
        let options = Rc::new(TestConfigOptions::new(vec![
            acp::SessionConfigOption::select(
                "mode",
                "Mode",
                "auto",
                vec![
                    acp::SessionConfigSelectOption::new("auto", "Auto"),
                    acp::SessionConfigSelectOption::new("manual", "Manual"),
                ],
            ),
            acp::SessionConfigOption::boolean("enabled", "Enabled", true),
        ]));
        let shared: Rc<dyn AgentSessionConfigOptions> = options.clone();
        let mode = acp::SessionConfigId::new("mode");
        let enabled = acp::SessionConfigId::new("enabled");
        let manual = acp::SessionConfigValueId::new("manual");

        let has_boolean = |config_id: &acp::SessionConfigId| {
            shared.config_options().into_iter().any(|option| {
                &option.id == config_id && matches!(option.kind, acp::SessionConfigKind::Boolean(_))
            })
        };

        assert!(config_option_is_select(&shared, &mode));
        assert!(has_boolean(&enabled));
        assert!(config_option_has_value(&shared, &mode, &manual));

        *options.options.borrow_mut() = vec![acp::SessionConfigOption::select(
            "mode",
            "Mode",
            "auto",
            vec![acp::SessionConfigSelectOption::new("auto", "Auto")],
        )];

        assert!(config_option_is_select(&shared, &mode));
        assert!(!has_boolean(&enabled));
        assert!(!config_option_has_value(&shared, &mode, &manual));
    }

    #[derive(Default)]
    struct TestAgentServer {
        saved_defaults: Arc<Mutex<Vec<(String, Option<AgentConfigOptionValue>)>>>,
    }

    impl AgentServer for TestAgentServer {
        fn logo(&self) -> IconName {
            IconName::Sparkle
        }

        fn agent_id(&self) -> AgentId {
            AgentId::new("test-agent")
        }

        fn connect(
            &self,
            _delegate: AgentServerDelegate,
            _project: Entity<Project>,
            _cx: &mut App,
        ) -> Task<anyhow::Result<Rc<dyn AgentConnection>>> {
            Task::ready(Err(anyhow::anyhow!("test agent server cannot connect")))
        }

        fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
            self
        }

        fn set_default_config_option(
            &self,
            config_id: &str,
            value: Option<AgentConfigOptionValue>,
            _fs: Arc<dyn Fs>,
            _cx: &mut App,
        ) {
            self.saved_defaults
                .lock()
                .push((config_id.to_string(), value));
        }
    }
}
