use gpui::{
    App, Entity, EventEmitter, FocusHandle, Focusable, PromptButton, PromptHandle, PromptLevel,
    PromptResponse, RenderablePromptHandle, SharedString, TextStyleRefinement, Window, div,
    prelude::*,
};
use markdown::{Markdown, MarkdownElement, MarkdownStyle};
use settings::Settings;
use theme_settings::ThemeSettings;
use ui::{FluentBuilder, TintColor, prelude::*};

pub fn init(cx: &mut App) {
    cx.set_prompt_builder(zzz_prompt_renderer);
}

/// Use this function in conjunction with [App::set_prompt_builder] to force
/// GPUI to use the internal prompt system.
fn zzz_prompt_renderer(
    level: PromptLevel,
    message: &str,
    detail: Option<&str>,
    actions: &[PromptButton],
    handle: PromptHandle,
    window: &mut Window,
    cx: &mut App,
) -> RenderablePromptHandle {
    let renderer = cx.new({
        |cx| ZZZPromptRenderer {
            _level: level,
            message: cx.new(|cx| Markdown::new(SharedString::new(message), None, None, cx)),
            actions: actions
                .iter()
                .map(|button| button.label().to_string())
                .collect(),
            cancel_action_id: cancel_action_id(actions),
            focus: cx.focus_handle(),
            active_action_id: 0,
            detail: detail
                .filter(|text| !text.is_empty())
                .map(|text| cx.new(|cx| Markdown::new(SharedString::new(text), None, None, cx))),
        }
    });

    handle.with_view(renderer, window, cx)
}

/// Index of the button that answers the prompt with "cancelled", if it offers
/// one. Derived from the button kinds rather than their labels, which are
/// localized by the caller.
fn cancel_action_id(actions: &[PromptButton]) -> Option<usize> {
    actions.iter().position(PromptButton::is_cancel)
}

pub struct ZZZPromptRenderer {
    _level: PromptLevel,
    message: Entity<Markdown>,
    actions: Vec<String>,
    /// Index of the button that answers the prompt with "cancelled", so
    /// `menu::Cancel` does not have to compare against a label that may be
    /// localized.
    cancel_action_id: Option<usize>,
    focus: FocusHandle,
    active_action_id: usize,
    detail: Option<Entity<Markdown>>,
}

impl ZZZPromptRenderer {
    fn confirm(&mut self, _: &menu::Confirm, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(PromptResponse(self.active_action_id));
    }

    fn cancel(&mut self, _: &menu::Cancel, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(action_id) = self.cancel_action_id {
            cx.emit(PromptResponse(action_id));
        }
    }

    fn select_first(
        &mut self,
        _: &menu::SelectFirst,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Buttons render in `actions` order, so the first action is the topmost
        // one and `SelectNext`/`SelectPrevious` walk down and up the list.
        self.active_action_id = 0;
        cx.notify();
    }

    fn select_last(&mut self, _: &menu::SelectLast, _window: &mut Window, cx: &mut Context<Self>) {
        self.active_action_id = self.actions.len().saturating_sub(1);
        cx.notify();
    }

    fn select_next(&mut self, _: &menu::SelectNext, _window: &mut Window, cx: &mut Context<Self>) {
        if self.actions.is_empty() {
            return;
        }
        self.active_action_id = (self.active_action_id + 1) % self.actions.len();
        cx.notify();
    }

    fn select_previous(
        &mut self,
        _: &menu::SelectPrevious,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active_action_id > 0 {
            self.active_action_id -= 1;
        } else {
            self.active_action_id = self.actions.len().saturating_sub(1);
        }
        cx.notify();
    }
}

impl Render for ZZZPromptRenderer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = ThemeSettings::get_global(cx);

        let dialog = v_flex()
            .key_context("Prompt")
            .cursor_default()
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_last))
            .w_80()
            .p_4()
            .gap_4()
            .elevation_3(cx)
            .overflow_hidden()
            .font_family(settings.ui_font.family.clone())
            .child(div().w_full().child(MarkdownElement::new(
                self.message.clone(),
                markdown_style(true, window, cx),
            )))
            .children(self.detail.clone().map(|detail| {
                div().w_full().text_xs().child(MarkdownElement::new(
                    detail,
                    markdown_style(false, window, cx),
                ))
            }))
            .child(
                v_flex()
                    .gap_1()
                    .children(
                        self.actions
                            .iter()
                            .enumerate()
                            .map(|(action_index, action)| {
                                Button::new(action_index, action.clone())
                                    .full_width()
                                    .style(ButtonStyle::Outlined)
                                    .when(action_index == self.active_action_id, |s| {
                                        s.style(ButtonStyle::Tinted(TintColor::Accent))
                                    })
                                    .tab_index(action_index as isize)
                                    .on_click(cx.listener(move |_, _, _window, cx| {
                                        cx.emit(PromptResponse(action_index));
                                    }))
                            }),
                    ),
            );

        div()
            .size_full()
            .occlude()
            .bg(gpui::black().opacity(0.2))
            .child(
                v_flex()
                    .size_full()
                    .absolute()
                    .top_0()
                    .left_0()
                    .items_center()
                    .justify_center()
                    .child(dialog),
            )
    }
}

fn markdown_style(main_message: bool, window: &Window, cx: &App) -> MarkdownStyle {
    let mut base_text_style = window.text_style();
    let settings = ThemeSettings::get_global(cx);
    let font_size = settings.ui_font_size(cx).into();

    let color = if main_message {
        Color::Default.color(cx)
    } else {
        Color::Muted.color(cx)
    };

    base_text_style.refine(&TextStyleRefinement {
        font_family: Some(settings.ui_font.family.clone()),
        font_size: Some(font_size),
        color: Some(color),
        ..Default::default()
    });

    MarkdownStyle {
        base_text_style,
        selection_background_color: cx.theme().colors().element_selection_background,
        ..Default::default()
    }
}

impl EventEmitter<PromptResponse> for ZZZPromptRenderer {}

impl Focusable for ZZZPromptRenderer {
    fn focus_handle(&self, _: &crate::App) -> FocusHandle {
        self.focus.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::cancel_action_id;
    use gpui::PromptButton;

    #[test]
    fn cancel_action_id_finds_the_cancel_button_by_kind() {
        let actions = [
            PromptButton::ok("OK"),
            PromptButton::new("Later"),
            PromptButton::cancel("Cancel"),
        ];

        assert_eq!(cancel_action_id(&actions), Some(2));
    }

    #[test]
    fn cancel_action_id_ignores_localized_labels() {
        let actions = [PromptButton::ok("确定"), PromptButton::cancel("取消")];

        assert_eq!(cancel_action_id(&actions), Some(1));
    }

    #[test]
    fn cancel_action_id_is_none_without_a_cancel_button() {
        let actions = [PromptButton::ok("OK"), PromptButton::new("Later")];

        assert_eq!(cancel_action_id(&actions), None);
    }
}
