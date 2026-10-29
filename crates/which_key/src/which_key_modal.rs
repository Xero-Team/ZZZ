//! Modal implementation for the which-key display.

use gpui::prelude::FluentBuilder;
use gpui::{
    Action, App, Context, DismissEvent, EventEmitter, FocusHandle, Focusable, FontWeight,
    KeybindingKeystroke, ScrollHandle, Subscription, WeakEntity, Window,
};
use i18n::tr;
use settings::Settings;
use theme_settings::ThemeSettings;
use ui::{
    Divider, DividerColor, DynamicSpacing, LabelSize, WithScrollbar, prelude::*,
    text_for_keybinding_keystrokes, text_for_keystrokes,
};
use workspace::{ModalView, Workspace};

use crate::FILTERED_KEYSTROKES;

pub struct WhichKeyModal {
    _workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
    bindings: Vec<(SharedString, SharedString)>,
    pending_keys: SharedString,
    _pending_input_subscription: Subscription,
    _focus_out_subscription: Subscription,
}

fn binding_label(action: &dyn Action) -> SharedString {
    match action.as_any().downcast_ref::<zzz_actions::Spawn>() {
        Some(zzz_actions::Spawn::ByName { task_name, .. }) => task_name.clone().into(),
        _ => command_palette::humanize_action_name(action.name()).into(),
    }
}

impl WhichKeyModal {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // Keep focus where it currently is
        let focus_handle = window.focused(cx).unwrap_or(cx.focus_handle());

        let handle = cx.weak_entity();
        let mut this = Self {
            _workspace: workspace,
            focus_handle: focus_handle.clone(),
            scroll_handle: ScrollHandle::new(),
            bindings: Vec::new(),
            pending_keys: SharedString::new_static(""),
            _pending_input_subscription: cx.observe_pending_input(
                window,
                |this: &mut Self, window, cx| {
                    this.update_pending_keys(window, cx);
                },
            ),
            _focus_out_subscription: window.on_focus_out(&focus_handle, cx, move |_, _, cx| {
                if let Some(handle) = handle.upgrade() {
                    handle.update(cx, |_, cx| cx.emit(DismissEvent));
                }
            }),
        };
        this.update_pending_keys(window, cx);
        this
    }

    pub fn dismiss(&self, cx: &mut Context<Self>) {
        cx.emit(DismissEvent)
    }

    fn update_pending_keys(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pending_keys) = window.pending_input_keystrokes() else {
            cx.emit(DismissEvent);
            return;
        };
        let bindings = window.possible_bindings_for_input(pending_keys);

        let mut binding_data = bindings
            .iter()
            .map(|binding| (binding.keystrokes().to_vec(), binding.action()))
            .filter(|(keystrokes, _action)| {
                // Check if this binding matches any filtered keystroke pattern
                !FILTERED_KEYSTROKES.iter().any(|filtered| {
                    keystrokes.len() >= filtered.len()
                        && keystrokes[..filtered.len()]
                            .iter()
                            .map(|keystroke| keystroke.inner())
                            .eq(filtered.iter())
                })
            })
            .filter_map(|(keystrokes, action)| {
                let remaining_keystrokes = keystrokes.get(pending_keys.len()..)?.to_vec();
                let action_name = binding_label(action);
                Some((remaining_keystrokes, action_name))
            })
            .collect();

        let group_label = tr(cx, "which_key.group_keybinds", "+{} keybinds");
        binding_data = group_bindings(binding_data, &group_label);

        // Sort bindings from shortest to longest, with groups last. Groups are
        // the actions whose label starts with "+". `sort_by_cached_key` keeps the
        // sort stable and formats each keystroke label only once.
        binding_data.sort_by_cached_key(|(keystrokes, action_name)| {
            let is_group = action_name.starts_with('+');
            let label = text_for_keybinding_keystrokes(keystrokes, cx);
            (is_group, keystrokes.len(), label.len(), label)
        });
        binding_data.dedup();
        self.pending_keys = text_for_keystrokes(&pending_keys, cx).into();
        self.bindings = binding_data
            .into_iter()
            .map(|(keystrokes, action)| {
                (
                    text_for_keybinding_keystrokes(&keystrokes, cx).into(),
                    action,
                )
            })
            .collect();
    }
}

impl Render for WhichKeyModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_rows = !self.bindings.is_empty();
        let viewport_size = window.viewport_size();

        let max_panel_width = px((f32::from(viewport_size.width) * 0.5).min(480.0));
        let max_content_height = px(f32::from(viewport_size.height) * 0.4);

        // Push above status bar when visible
        let status_height = self
            ._workspace
            .upgrade()
            .and_then(|workspace| {
                workspace.read_with(cx, |workspace, cx| {
                    if workspace.status_bar_visible(cx) {
                        Some(
                            DynamicSpacing::Base04.px(cx) * 2.0
                                + ThemeSettings::get_global(cx).ui_font_size(cx),
                        )
                    } else {
                        None
                    }
                })
            })
            .unwrap_or(px(0.));

        let margin_bottom = px(16.);
        let bottom_offset = margin_bottom + status_height;

        // Title section
        let title_section = {
            let mut column = v_flex().gap(px(0.)).child(
                div()
                    .child(
                        Label::new(self.pending_keys.clone())
                            .size(LabelSize::Default)
                            .weight(FontWeight::MEDIUM)
                            .color(Color::Accent),
                    )
                    .mb(px(2.)),
            );

            if has_rows {
                column = column.child(
                    div()
                        .child(Divider::horizontal().color(DividerColor::BorderFaded))
                        .mb(px(2.)),
                );
            }

            column
        };

        let content = h_flex()
            .items_start()
            .id("which-key-content")
            .gap(px(8.))
            .overflow_y_scroll()
            .track_scroll(&self.scroll_handle)
            .h_full()
            .max_h(max_content_height)
            .child(
                // Keystrokes column
                v_flex()
                    .gap(px(4.))
                    .flex_shrink_0()
                    .children(self.bindings.iter().map(|(keystrokes, _)| {
                        div()
                            .child(
                                Label::new(keystrokes.clone())
                                    .size(LabelSize::Default)
                                    .color(Color::Accent),
                            )
                            .text_align(gpui::TextAlign::Right)
                    })),
            )
            .child(
                // Actions column
                v_flex()
                    .gap(px(4.))
                    .flex_1()
                    .min_w_0()
                    .children(self.bindings.iter().map(|(_, action_name)| {
                        let is_group = action_name.starts_with('+');
                        let label_color = if is_group {
                            Color::Success
                        } else {
                            Color::Default
                        };

                        div().child(
                            Label::new(action_name.clone())
                                .size(LabelSize::Default)
                                .color(label_color)
                                .single_line()
                                .truncate(),
                        )
                    })),
            );

        div()
            .id("which-key-buffer-panel-scroll")
            .occlude()
            .absolute()
            .bottom(bottom_offset)
            .right(px(16.))
            .min_w(px(220.))
            .max_w(max_panel_width)
            .elevation_3(cx)
            .px(px(12.))
            .child(v_flex().child(title_section).when(has_rows, |el| {
                el.child(
                    div()
                        .max_h(max_content_height)
                        .child(content)
                        .vertical_scrollbar_for(&self.scroll_handle, window, cx),
                )
            }))
    }
}

impl EventEmitter<DismissEvent> for WhichKeyModal {}

impl Focusable for WhichKeyModal {
    fn focus_handle(&self, _cx: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl ModalView for WhichKeyModal {
    fn render_bare(&self) -> bool {
        true
    }
}

fn group_bindings(
    binding_data: Vec<(Vec<KeybindingKeystroke>, SharedString)>,
    group_label: &str,
) -> Vec<(Vec<KeybindingKeystroke>, SharedString)> {
    let mut groups: Vec<Vec<(Vec<KeybindingKeystroke>, SharedString)>> = Vec::new();

    // Keep the candidate order from GPUI. When multiple bindings have the
    // same sequence, dispatch tries the first one and the later bindings are
    // shadowed.
    for (remaining_keystrokes, action_name) in binding_data {
        let first_key = remaining_keystrokes.first();
        if let Some(group) = groups.iter_mut().find(|group| {
            group
                .first()
                .is_some_and(|(keystrokes, _)| keystrokes.first() == first_key)
        }) {
            if group
                .iter()
                .any(|(keystrokes, _)| *keystrokes == remaining_keystrokes)
            {
                continue;
            }
            group.push((remaining_keystrokes, action_name));
        } else {
            groups.push(vec![(remaining_keystrokes, action_name)]);
        }
    }

    let mut result = Vec::new();

    for group in groups {
        let longer_count = group
            .iter()
            .filter(|(keystrokes, _)| keystrokes.len() > 1)
            .count();

        if longer_count > 1
            && let Some(first_key) = group
                .first()
                .and_then(|(keystrokes, _)| keystrokes.first())
                .cloned()
        {
            // A group row must not hide the action that the next keystroke
            // completes. Keep that action visible and collapse only longer
            // continuations.
            result.extend(
                group
                    .into_iter()
                    .filter(|(keystrokes, _)| keystrokes.len() <= 1),
            );
            result.push((
                vec![first_key],
                group_label.replace("{}", &longer_count.to_string()).into(),
            ));
        } else {
            result.extend(group);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_keystrokes(sequence: &str) -> Vec<KeybindingKeystroke> {
        sequence
            .split(' ')
            .map(|part| {
                KeybindingKeystroke::from_keystroke(
                    Keystroke::parse(part).expect("valid keystroke"),
                )
            })
            .collect()
    }

    #[test]
    fn group_bindings_collapses_shared_prefixes_and_deduplicates() {
        let grouped = group_bindings(
            vec![
                (parse_keystrokes("a b"), "Action One".into()),
                (parse_keystrokes("a c"), "Action Two".into()),
                (parse_keystrokes("a b"), "Duplicate Action".into()),
                (parse_keystrokes("z"), "Standalone".into()),
            ],
            "+{} keybinds",
        );

        assert!(grouped.contains(&(parse_keystrokes("a"), "+2 keybinds".into())));
        assert!(grouped.contains(&(parse_keystrokes("z"), "Standalone".into())));
        assert_eq!(grouped.len(), 2);
    }

    #[test]
    fn group_bindings_shows_completed_binding_before_continuations() {
        let grouped = group_bindings(
            vec![
                (parse_keystrokes("a"), "Complete".into()),
                (parse_keystrokes("a b"), "Continue One".into()),
                (parse_keystrokes("a c"), "Continue Two".into()),
            ],
            "+{} keybinds",
        );

        assert_eq!(
            grouped,
            vec![
                (parse_keystrokes("a"), "Complete".into()),
                (parse_keystrokes("a"), "+2 keybinds".into()),
            ]
        );
    }

    #[test]
    fn group_bindings_keeps_the_first_binding_for_duplicate_sequences() {
        let grouped = group_bindings(
            vec![
                (parse_keystrokes("a"), "First".into()),
                (parse_keystrokes("a b"), "Continuation".into()),
                (parse_keystrokes("a"), "Shadowed".into()),
            ],
            "+{} keybinds",
        );

        assert_eq!(
            grouped,
            vec![
                (parse_keystrokes("a"), "First".into()),
                (parse_keystrokes("a b"), "Continuation".into()),
            ]
        );
    }

    #[cfg(target_os = "windows")]
    use gpui::Modifiers;
    use gpui::{InvalidKeystrokeError, Keystroke};

    #[test]
    fn test_group_bindings_preserves_keybinding_keystrokes() -> Result<(), InvalidKeystrokeError> {
        #[cfg(target_os = "windows")]
        let keystroke = KeybindingKeystroke::new(
            Keystroke::parse("ctrl-$")?,
            Modifiers::control_shift(),
            "4".to_owned(),
        );
        #[cfg(not(target_os = "windows"))]
        let keystroke = KeybindingKeystroke::from_keystroke(Keystroke::parse("ctrl-x")?);
        let binding_data = vec![(vec![keystroke.clone()], SharedString::from("test action"))];

        let grouped_bindings = group_bindings(binding_data, "+{} keybinds");

        assert_eq!(
            grouped_bindings,
            vec![(vec![keystroke], SharedString::from("test action"))]
        );
        Ok(())
    }

    #[test]
    fn test_binding_label_uses_task_name_for_spawn_by_name() {
        let action = zzz_actions::Spawn::ByName {
            task_name: "lazygit".to_string(),
            reveal_target: None,
        };

        assert_eq!(binding_label(&action), "lazygit");
    }
}
