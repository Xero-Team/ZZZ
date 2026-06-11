#![allow(unused, dead_code)]
use gpui::{
    AnyElement, App, Entity, EventEmitter, FocusHandle, Focusable, Hsla, Task, actions, hsla,
};
use i18n::tr;
use strum::IntoEnumIterator;
use theme::all_theme_colors;
use ui::{
    AudioStatus, Avatar, AvatarAudioStatusIndicator, AvatarAvailabilityIndicator, ButtonLike,
    Checkbox, CollaboratorAvailability, DecoratedIcon, ElevationIndex, Facepile, IconDecoration,
    Indicator, KeybindingHint, Switch, TintColor, Tooltip, prelude::*,
    utils::calculate_contrast_ratio,
};

use crate::{Item, Workspace};

actions!(
    dev,
    [
        /// Opens the theme preview window.
        OpenThemePreview
    ]
);

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &OpenThemePreview, window, cx| {
            let theme_preview = cx.new(|cx| ThemePreview::new(window, cx));
            workspace.add_item_to_active_pane(Box::new(theme_preview), None, true, window, cx)
        });
    })
    .detach();
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, strum::EnumIter)]
enum ThemePreviewPage {
    Overview,
    Typography,
}

impl ThemePreviewPage {
    pub fn id(&self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Typography => "Typography",
        }
    }

    pub fn name(&self, cx: &App) -> String {
        match self {
            Self::Overview => tr(cx, "workspace.theme_preview.page.overview", "Overview"),
            Self::Typography => tr(cx, "workspace.theme_preview.page.typography", "Typography"),
        }
    }
}

struct ThemePreview {
    current_page: ThemePreviewPage,
    focus_handle: FocusHandle,
}

impl ThemePreview {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            current_page: ThemePreviewPage::Overview,
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn view(
        &self,
        page: ThemePreviewPage,
        window: &mut Window,
        cx: &mut Context<ThemePreview>,
    ) -> impl IntoElement {
        match page {
            ThemePreviewPage::Overview => self.render_overview_page(window, cx).into_any_element(),
            ThemePreviewPage::Typography => {
                self.render_typography_page(window, cx).into_any_element()
            }
        }
    }
}

impl EventEmitter<()> for ThemePreview {}

impl Focusable for ThemePreview {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}
impl ThemePreview {}

impl Item for ThemePreview {
    type Event = ();

    fn to_item_events(_: &Self::Event, _: &mut dyn FnMut(crate::item::ItemEvent)) {}

    fn tab_content_text(&self, _detail: usize, cx: &App) -> SharedString {
        let name = cx.theme().name.clone();
        tr(cx, "workspace.theme_preview.tab_title", "{} Preview")
            .replacen("{}", name.as_ref(), 1)
            .into()
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        None
    }

    fn can_split(&self) -> bool {
        true
    }

    fn clone_on_split(
        &self,
        _workspace_id: Option<crate::WorkspaceId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Option<Entity<Self>>>
    where
        Self: Sized,
    {
        Task::ready(Some(cx.new(|cx| Self::new(window, cx))))
    }
}

const AVATAR_URL: &str = "https://avatars.githubusercontent.com/u/1714999?v=4";

impl ThemePreview {
    fn layer_name(layer: ElevationIndex, cx: &App) -> String {
        match layer {
            ElevationIndex::Background => {
                tr(cx, "workspace.theme_preview.layer.background", "Background")
            }
            ElevationIndex::Surface => tr(cx, "workspace.theme_preview.layer.surface", "Surface"),
            ElevationIndex::EditorSurface => tr(
                cx,
                "workspace.theme_preview.layer.editor_surface",
                "Editor Surface",
            ),
            ElevationIndex::ElevatedSurface => tr(
                cx,
                "workspace.theme_preview.layer.elevated_surface",
                "Elevated Surface",
            ),
            ElevationIndex::ModalSurface => tr(
                cx,
                "workspace.theme_preview.layer.modal_surface",
                "Modal Surface",
            ),
        }
    }

    fn preview_bg(window: &mut Window, cx: &mut App) -> Hsla {
        cx.theme().colors().editor_background
    }

    fn render_text(
        &self,
        layer: ElevationIndex,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let bg = layer.bg(cx);
        let lt = |key: &str, fallback: &str| tr(cx, key, fallback);

        let label_with_contrast = |label: &str, fg: Hsla| {
            let contrast = calculate_contrast_ratio(fg, bg);
            format!("{} ({:.2})", label, contrast)
        };

        v_flex()
            .gap_1()
            .child(
                Headline::new(lt("workspace.theme_preview.text.title", "Text"))
                    .size(HeadlineSize::Small)
                    .color(Color::Muted),
            )
            .child(
                h_flex()
                    .items_start()
                    .gap_4()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                Headline::new(lt(
                                    "workspace.theme_preview.headline_sizes",
                                    "Headline Sizes",
                                ))
                                .size(HeadlineSize::Small)
                                .color(Color::Muted),
                            )
                            .child(Headline::new(lt(
                                "workspace.theme_preview.headline.xlarge",
                                "XLarge Headline",
                            ))
                            .size(HeadlineSize::XLarge))
                            .child(Headline::new(lt(
                                "workspace.theme_preview.headline.large",
                                "Large Headline",
                            ))
                            .size(HeadlineSize::Large))
                            .child(Headline::new(lt(
                                "workspace.theme_preview.headline.medium",
                                "Medium Headline",
                            ))
                            .size(HeadlineSize::Medium))
                            .child(Headline::new(lt(
                                "workspace.theme_preview.headline.small",
                                "Small Headline",
                            ))
                            .size(HeadlineSize::Small))
                            .child(Headline::new(lt(
                                "workspace.theme_preview.headline.xsmall",
                                "XSmall Headline",
                            ))
                            .size(HeadlineSize::XSmall)),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                Headline::new(lt(
                                    "workspace.theme_preview.text_colors",
                                    "Text Colors",
                                ))
                                .size(HeadlineSize::Small)
                                .color(Color::Muted),
                            )
                            .child(
                                Label::new(label_with_contrast(
                                    &lt(
                                        "workspace.theme_preview.color.default_text",
                                        "Default Text",
                                    ),
                                    Color::Default.color(cx),
                                ))
                                .color(Color::Default),
                            )
                            .child(
                                Label::new(label_with_contrast(
                                    &lt(
                                        "workspace.theme_preview.color.accent_text",
                                        "Accent Text",
                                    ),
                                    Color::Accent.color(cx),
                                ))
                                .color(Color::Accent),
                            )
                            .child(
                                Label::new(label_with_contrast(
                                    &lt(
                                        "workspace.theme_preview.color.conflict_text",
                                        "Conflict Text",
                                    ),
                                    Color::Conflict.color(cx),
                                ))
                                .color(Color::Conflict),
                            )
                            .child(
                                Label::new(label_with_contrast(
                                    &lt(
                                        "workspace.theme_preview.color.created_text",
                                        "Created Text",
                                    ),
                                    Color::Created.color(cx),
                                ))
                                .color(Color::Created),
                            )
                            .child(
                                Label::new(label_with_contrast(
                                    &lt(
                                        "workspace.theme_preview.color.deleted_text",
                                        "Deleted Text",
                                    ),
                                    Color::Deleted.color(cx),
                                ))
                                .color(Color::Deleted),
                            )
                            .child(
                                Label::new(label_with_contrast(
                                    &lt(
                                        "workspace.theme_preview.color.disabled_text",
                                        "Disabled Text",
                                    ),
                                    Color::Disabled.color(cx),
                                ))
                                .color(Color::Disabled),
                            )
                            .child(
                                Label::new(label_with_contrast(
                                    &lt(
                                        "workspace.theme_preview.color.error_text",
                                        "Error Text",
                                    ),
                                    Color::Error.color(cx),
                                ))
                                .color(Color::Error),
                            )
                            .child(
                                Label::new(label_with_contrast(
                                    &lt(
                                        "workspace.theme_preview.color.hidden_text",
                                        "Hidden Text",
                                    ),
                                    Color::Hidden.color(cx),
                                ))
                                .color(Color::Hidden),
                            )
                            .child(
                                Label::new(label_with_contrast(
                                    &lt(
                                        "workspace.theme_preview.color.hint_text",
                                        "Hint Text",
                                    ),
                                    Color::Hint.color(cx),
                                ))
                                .color(Color::Hint),
                            )
                            .child(
                                Label::new(label_with_contrast(
                                    &lt(
                                        "workspace.theme_preview.color.ignored_text",
                                        "Ignored Text",
                                    ),
                                    Color::Ignored.color(cx),
                                ))
                                .color(Color::Ignored),
                            )
                            .child(
                                Label::new(label_with_contrast(
                                    &lt(
                                        "workspace.theme_preview.color.info_text",
                                        "Info Text",
                                    ),
                                    Color::Info.color(cx),
                                ))
                                .color(Color::Info),
                            )
                            .child(
                                Label::new(label_with_contrast(
                                    &lt(
                                        "workspace.theme_preview.color.modified_text",
                                        "Modified Text",
                                    ),
                                    Color::Modified.color(cx),
                                ))
                                .color(Color::Modified),
                            )
                            .child(
                                Label::new(label_with_contrast(
                                    &lt(
                                        "workspace.theme_preview.color.muted_text",
                                        "Muted Text",
                                    ),
                                    Color::Muted.color(cx),
                                ))
                                .color(Color::Muted),
                            )
                            .child(
                                Label::new(label_with_contrast(
                                    &lt(
                                        "workspace.theme_preview.color.placeholder_text",
                                        "Placeholder Text",
                                    ),
                                    Color::Placeholder.color(cx),
                                ))
                                .color(Color::Placeholder),
                            )
                            .child(
                                Label::new(label_with_contrast(
                                    &lt(
                                        "workspace.theme_preview.color.selected_text",
                                        "Selected Text",
                                    ),
                                    Color::Selected.color(cx),
                                ))
                                .color(Color::Selected),
                            )
                            .child(
                                Label::new(label_with_contrast(
                                    &lt(
                                        "workspace.theme_preview.color.success_text",
                                        "Success Text",
                                    ),
                                    Color::Success.color(cx),
                                ))
                                .color(Color::Success),
                            )
                            .child(
                                Label::new(label_with_contrast(
                                    &lt(
                                        "workspace.theme_preview.color.warning_text",
                                        "Warning Text",
                                    ),
                                    Color::Warning.color(cx),
                                ))
                                .color(Color::Warning),
                            )
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                Headline::new(lt(
                                    "workspace.theme_preview.wrapping_text",
                                    "Wrapping Text",
                                ))
                                .size(HeadlineSize::Small)
                                .color(Color::Muted),
                            )
                            .child(div().max_w(px(200.)).child(lt(
                                "workspace.theme_preview.wrapping_text.sample",
                                "This is a longer piece of text that should wrap to multiple lines. It demonstrates how text behaves when it exceeds the width of its container.",
                            )))
                    )
            )
    }

    fn render_colors(
        &self,
        layer: ElevationIndex,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let bg = layer.bg(cx);
        let all_colors = all_theme_colors(cx);

        v_flex()
            .gap_1()
            .child(
                Headline::new(tr(cx, "workspace.theme_preview.colors", "Colors"))
                    .size(HeadlineSize::Small)
                    .color(Color::Muted),
            )
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_1()
                    .children(all_colors.into_iter().map(|(color, name)| {
                        let id = ElementId::Name(format!("{:?}-preview", color).into());
                        div().size_8().flex_none().child(
                            ButtonLike::new(id)
                                .child(
                                    div()
                                        .size_8()
                                        .bg(color)
                                        .border_1()
                                        .border_color(cx.theme().colors().border)
                                        .overflow_hidden(),
                                )
                                .size(ButtonSize::None)
                                .style(ButtonStyle::Transparent)
                                .tooltip(move |window, cx| {
                                    let name = name.clone();
                                    Tooltip::with_meta(name, None, format!("{:?}", color), cx)
                                }),
                        )
                    })),
            )
    }

    fn render_theme_layer(
        &self,
        layer: ElevationIndex,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        v_flex()
            .p_4()
            .bg(layer.bg(cx))
            .text_color(cx.theme().colors().text)
            .gap_2()
            .child(Headline::new(Self::layer_name(layer, cx)).size(HeadlineSize::Medium))
            .child(self.render_text(layer, window, cx))
            .child(self.render_colors(layer, window, cx))
    }

    fn render_overview_page(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        v_flex()
            .id("theme-preview-overview")
            .overflow_scroll()
            .size_full()
            .child(
                v_flex()
                    .child(
                        Headline::new(tr(
                            cx,
                            "workspace.theme_preview.title",
                            "Theme Preview",
                        ))
                        .size(HeadlineSize::Large),
                    )
                    .child(
                        div()
                            .w_full()
                            .text_color(cx.theme().colors().text_muted)
                            .child(tr(
                                cx,
                                "workspace.theme_preview.description",
                                "This view lets you preview a range of UI elements across a theme. Use it for testing out changes to the theme.",
                            )),
                    ),
            )
            .child(self.render_theme_layer(ElevationIndex::Background, window, cx))
            .child(self.render_theme_layer(ElevationIndex::Surface, window, cx))
            .child(self.render_theme_layer(ElevationIndex::EditorSurface, window, cx))
            .child(self.render_theme_layer(ElevationIndex::ElevatedSurface, window, cx))
    }

    fn render_typography_page(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let lt = |key: &str, fallback: &str| tr(cx, key, fallback);
        v_flex()
            .id("theme-preview-typography")
            .overflow_scroll()
            .size_full()
            .child(
                v_flex()
                    .gap_4()
                    .child(
                        Headline::new(lt(
                            "workspace.theme_preview.typography.headline_1",
                            "Headline 1",
                        ))
                        .size(HeadlineSize::XLarge),
                    )
                    .child(Label::new(lt(
                        "workspace.theme_preview.typography.sample_1",
                        "A strong display headline gives the theme room to show hierarchy, spacing, and rhythm at a glance.",
                    )))
                    .child(
                        Headline::new(lt(
                            "workspace.theme_preview.typography.headline_2",
                            "Headline 2",
                        ))
                        .size(HeadlineSize::Large),
                    )
                    .child(Label::new(lt(
                        "workspace.theme_preview.typography.sample_2",
                        "Secondary headings should still feel clear and confident, even when the surrounding interface is dense.",
                    )))
                    .child(
                        Headline::new(lt(
                            "workspace.theme_preview.typography.headline_3",
                            "Headline 3",
                        ))
                        .size(HeadlineSize::Medium),
                    )
                    .child(Label::new(lt(
                        "workspace.theme_preview.typography.sample_3",
                        "Medium headings often sit next to controls, status text, and lists, so balance matters.",
                    )))
                    .child(
                        Headline::new(lt(
                            "workspace.theme_preview.typography.headline_4",
                            "Headline 4",
                        ))
                        .size(HeadlineSize::Small),
                    )
                    .child(Label::new(lt(
                        "workspace.theme_preview.typography.sample_4",
                        "Smaller headings should remain readable without overpowering nearby body text.",
                    )))
                    .child(
                        Headline::new(lt(
                            "workspace.theme_preview.typography.headline_5",
                            "Headline 5",
                        ))
                        .size(HeadlineSize::XSmall),
                    )
                    .child(Label::new(lt(
                        "workspace.theme_preview.typography.sample_5",
                        "Compact headline styles are useful for side panels, forms, and grouped settings.",
                    )))
                    .child(
                        Headline::new(lt(
                            "workspace.theme_preview.typography.body_text",
                            "Body Text",
                        ))
                        .size(HeadlineSize::Small),
                    )
                    .child(Label::new(lt(
                        "workspace.theme_preview.typography.body_sample",
                        "Body copy should stay comfortable over long reads, preserve contrast across surfaces, and remain stable beside code, labels, and controls.",
                    ))),
            )
    }

    fn render_page_nav(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .id("theme-preview-nav")
            .items_center()
            .gap_4()
            .py_2()
            .bg(Self::preview_bg(window, cx))
            .children(ThemePreviewPage::iter().map(|p| {
                let label = p.name(cx);
                Button::new(ElementId::Name(p.id().into()), label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.current_page = p;
                        cx.notify();
                    }))
                    .toggle_state(p == self.current_page)
                    .selected_style(ButtonStyle::Tinted(TintColor::Accent))
            }))
    }
}

impl Render for ThemePreview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl ui::IntoElement {
        v_flex()
            .id("theme-preview")
            .key_context("ThemePreview")
            .items_start()
            .overflow_hidden()
            .size_full()
            .max_h_full()
            .track_focus(&self.focus_handle)
            .px_2()
            .bg(Self::preview_bg(window, cx))
            .child(self.render_page_nav(window, cx))
            .child(self.view(self.current_page, window, cx))
    }
}
