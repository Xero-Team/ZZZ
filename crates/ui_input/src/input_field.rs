use component::{example_group, single_example};

use gpui::{App, FocusHandle, Focusable, Hsla, Length, Subscription};
use i18n::tr;
use parking_lot::Mutex;
use std::{any::Any, sync::Arc};

use ui::Tooltip;
use ui::prelude::*;

use crate::{EditorFactoryUnavailable, ErasedEditor, create_editor};

pub struct InputFieldStyle {
    text_color: Hsla,
    background_color: Hsla,
    border_color: Hsla,
}

/// An Input Field component that can be used to create text fields like search inputs, form fields, etc.
///
/// It wraps a single line [`Editor`] and allows for common field properties like labels, placeholders, icons, etc.
#[derive(RegisterComponent)]
pub struct InputField {
    /// An optional label for the text field.
    ///
    /// Its position is determined by the [`FieldLabelLayout`].
    label: Option<SharedString>,
    /// The size of the label text.
    label_size: LabelSize,
    /// The placeholder text for the text field.
    placeholder: SharedString,

    editor: Arc<dyn ErasedEditor>,
    /// An optional icon that is displayed at the start of the text field.
    ///
    /// For example, a magnifying glass icon in a search field.
    start_icon: Option<IconName>,
    /// The minimum width of for the input
    min_width: Length,
    /// The tab index for keyboard navigation order.
    tab_index: Option<isize>,
    /// Whether this field is a tab stop (can be focused via Tab key).
    tab_stop: bool,
    /// Whether the field content is masked (for sensitive fields like passwords or API keys).
    masked: Option<bool>,
}

impl Focusable for InputField {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.focus_handle(cx)
    }
}

impl InputField {
    pub fn new(window: &mut Window, cx: &mut App, placeholder_text: &str) -> Self {
        Self::try_new(window, cx, placeholder_text).unwrap_or_else(|error| {
            log::error!("failed to create InputField editor: {error}");
            Self::from_editor(
                Arc::new(UnavailableEditor::new(cx, placeholder_text)),
                window,
                cx,
                placeholder_text,
            )
        })
    }

    pub fn try_new(
        window: &mut Window,
        cx: &mut App,
        placeholder_text: &str,
    ) -> Result<Self, EditorFactoryUnavailable> {
        let editor = create_editor(window, cx)?;
        Ok(Self::from_editor(editor, window, cx, placeholder_text))
    }

    fn from_editor(
        editor: Arc<dyn ErasedEditor>,
        window: &mut Window,
        cx: &mut App,
        placeholder_text: &str,
    ) -> Self {
        editor.set_placeholder_text(placeholder_text, window, cx);

        InputField {
            label: None,
            label_size: LabelSize::Small,
            placeholder: SharedString::new(placeholder_text),
            editor,
            start_icon: None,
            min_width: px(192.).into(),
            tab_index: None,
            tab_stop: true,
            masked: None,
        }
    }

    pub fn start_icon(mut self, icon: IconName) -> Self {
        self.start_icon = Some(icon);
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn label_size(mut self, size: LabelSize) -> Self {
        self.label_size = size;
        self
    }

    pub fn label_min_width(mut self, width: impl Into<Length>) -> Self {
        self.min_width = width.into();
        self
    }

    pub fn tab_index(mut self, index: isize) -> Self {
        self.tab_index = Some(index);
        self
    }

    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }

    /// Sets this field as a masked/sensitive input (e.g., for passwords or API keys).
    pub fn masked(mut self, masked: bool) -> Self {
        self.masked = Some(masked);
        self
    }

    pub fn is_empty(&self, cx: &App) -> bool {
        self.editor().text(cx).trim().is_empty()
    }

    pub fn editor(&self) -> &Arc<dyn ErasedEditor> {
        &self.editor
    }

    pub fn text(&self, cx: &App) -> String {
        self.editor().text(cx)
    }

    pub fn clear(&self, window: &mut Window, cx: &mut App) {
        self.editor().clear(window, cx)
    }

    pub fn set_text(&self, text: &str, window: &mut Window, cx: &mut App) {
        self.editor().set_text(text, window, cx)
    }

    pub fn set_masked(&self, masked: bool, window: &mut Window, cx: &mut App) {
        self.editor().set_masked(masked, window, cx)
    }
}

struct UnavailableEditor {
    focus_handle: FocusHandle,
    text: Mutex<String>,
    placeholder: Mutex<String>,
}

impl UnavailableEditor {
    fn new(cx: &App, placeholder: &str) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            text: Mutex::new(String::new()),
            placeholder: Mutex::new(placeholder.to_owned()),
        }
    }
}

impl ErasedEditor for UnavailableEditor {
    fn text(&self, _: &App) -> String {
        self.text.lock().clone()
    }

    fn set_text(&self, text: &str, _: &mut Window, _: &mut App) {
        *self.text.lock() = text.to_owned();
    }

    fn clear(&self, _: &mut Window, _: &mut App) {
        self.text.lock().clear();
    }

    fn set_placeholder_text(&self, text: &str, _: &mut Window, _: &mut App) {
        *self.placeholder.lock() = text.to_owned();
    }

    fn move_selection_to_end(&self, _: &mut Window, _: &mut App) {}

    fn select_all(&self, _: &mut Window, _: &mut App) {}

    fn set_masked(&self, _: bool, _: &mut Window, _: &mut App) {}

    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }

    fn subscribe(
        &self,
        _: Box<dyn FnMut(crate::ErasedEditorEvent, &mut Window, &mut App) + 'static>,
        _: &mut Window,
        _: &mut App,
    ) -> Subscription {
        Subscription::new(|| {})
    }

    fn render(&self, _: &mut Window, _: &App) -> AnyElement {
        let text = self.text.lock();
        let display_text = if text.is_empty() {
            self.placeholder.lock().clone()
        } else {
            text.clone()
        };
        div()
            .track_focus(&self.focus_handle)
            .child(display_text)
            .into_any_element()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Render for InputField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = self.editor.clone();

        if let Some(masked) = self.masked {
            self.editor.set_masked(masked, window, cx);
        }

        let theme_color = cx.theme().colors();

        let style = InputFieldStyle {
            text_color: theme_color.text,
            background_color: theme_color.editor_background,
            border_color: theme_color.border_variant,
        };

        let focus_handle = self.editor.focus_handle(cx);

        let configured_handle = if let Some(tab_index) = self.tab_index {
            focus_handle.tab_index(tab_index).tab_stop(self.tab_stop)
        } else if !self.tab_stop {
            focus_handle.tab_stop(false)
        } else {
            focus_handle
        };

        v_flex()
            .id(self.placeholder.clone())
            .w_full()
            .gap_1()
            .when_some(self.label.clone(), |this, label| {
                this.child(
                    Label::new(label)
                        .size(self.label_size)
                        .color(Color::Default),
                )
            })
            .child(
                h_flex()
                    .track_focus(&configured_handle)
                    .min_w(self.min_width)
                    .min_h_8()
                    .w_full()
                    .px_2()
                    .py_1p5()
                    .flex_grow()
                    .text_color(style.text_color)
                    .rounded_md()
                    .bg(style.background_color)
                    .border_1()
                    .border_color(style.border_color)
                    .when(
                        editor.focus_handle(cx).contains_focused(window, cx),
                        |this| this.border_color(theme_color.border_focused),
                    )
                    .when_some(self.start_icon, |this, icon| {
                        this.gap_1()
                            .child(Icon::new(icon).size(IconSize::Small).color(Color::Muted))
                    })
                    .child(self.editor.render(window, cx))
                    .when_some(self.masked, |this, is_masked| {
                        this.child(
                            IconButton::new(
                                "toggle-masked",
                                if is_masked {
                                    IconName::Eye
                                } else {
                                    IconName::EyeOff
                                },
                            )
                            .icon_size(IconSize::Small)
                            .icon_color(Color::Muted)
                            .tooltip(Tooltip::text(if is_masked {
                                tr(cx, "ui_input.show", "Show")
                            } else {
                                tr(cx, "ui_input.hide", "Hide")
                            }))
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    if let Some(ref mut masked) = this.masked {
                                        *masked = !*masked;
                                        this.editor.set_masked(*masked, window, cx);
                                        cx.notify();
                                    }
                                },
                            )),
                        )
                    }),
            )
    }
}

impl Component for InputField {
    fn scope() -> ComponentScope {
        ComponentScope::Input
    }

    fn preview(window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        let input_small =
            cx.new(|cx| InputField::new(window, cx, "placeholder").label("Small Label"));

        let input_regular = cx.new(|cx| {
            InputField::new(window, cx, "placeholder")
                .label("Regular Label")
                .label_size(LabelSize::Default)
        });

        Some(
            v_flex()
                .gap_6()
                .children(vec![example_group(vec![
                    single_example(
                        "Small Label (Default)",
                        div().child(input_small).into_any_element(),
                    ),
                    single_example(
                        "Regular Label",
                        div().child(input_regular).into_any_element(),
                    ),
                ])])
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    struct EmptyView;

    impl Render for EmptyView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    #[gpui::test]
    fn missing_editor_factory_returns_error_and_uses_non_panicking_fallback(
        cx: &mut TestAppContext,
    ) {
        let window = cx.add_window(|_, _| EmptyView);
        window
            .update(cx, |_, window, cx| {
                assert_eq!(
                    InputField::try_new(window, cx, "placeholder").err(),
                    Some(EditorFactoryUnavailable)
                );

                let field = InputField::new(window, cx, "placeholder");
                assert_eq!(field.text(cx), "");
                field.set_text("fallback text", window, cx);
                assert_eq!(field.text(cx), "fallback text");
                field.clear(window, cx);
                assert_eq!(field.text(cx), "");
            })
            .expect("test window should remain open");
    }
}
