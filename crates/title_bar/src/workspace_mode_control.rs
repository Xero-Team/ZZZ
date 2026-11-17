//! The workspace mode control shown in the title bar.
//!
//! Adapted from Glass's workspace_chrome (GPL-3.0-or-later) at
//! e8ec0832b4f47b9e1420730c7d5bf7f3bd83eda9. Glass also shipped a browser
//! sidebar; ZZZ keeps only the mode control, which arranges existing docks.

use gpui::{Action as _, App, RenderOnce, Window, div};
use ui::{Button, ButtonSize, ButtonStyle, Label, LabelSize, TintColor, prelude::*};
use workspace_modes::{
    ModeId, SwitchToEditorMode, SwitchToTerminalMode, SwitchToZenMode, active_mode,
};

/// A compact three-state control for the workspace layout mode. The control is
/// read-only: clicking a mode dispatches the corresponding workspace action, so
/// the layout logic stays in `workspace_modes`.
#[derive(IntoElement)]
pub struct ModeControl;

impl ModeControl {
    pub fn new() -> Self {
        Self
    }
}

impl RenderOnce for ModeControl {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let mode = active_mode(cx);

        h_flex()
            .gap_0p5()
            .children(
                [ModeId::Editor, ModeId::Terminal, ModeId::Zen]
                    .into_iter()
                    .map(|candidate| {
                        let selected = mode == candidate;
                        div().when(!selected, |this| this.opacity(0.6)).child(
                            Button::new(
                                format!("workspace-mode-{}", candidate.as_str()),
                                candidate.label(cx),
                            )
                            .style(if selected {
                                ButtonStyle::Tinted(TintColor::Accent)
                            } else {
                                ButtonStyle::Subtle
                            })
                            .size(ButtonSize::Compact)
                            .label_size(LabelSize::Small)
                            .on_click(move |_, window, cx| {
                                dispatch_mode(candidate, window, cx);
                            }),
                        )
                    }),
            )
            .child(Label::new("").size(LabelSize::Small))
    }
}

fn dispatch_mode(mode: ModeId, window: &mut Window, cx: &mut App) {
    match mode {
        ModeId::Editor => window.dispatch_action(SwitchToEditorMode.boxed_clone(), cx),
        ModeId::Terminal => window.dispatch_action(SwitchToTerminalMode.boxed_clone(), cx),
        ModeId::Zen => window.dispatch_action(SwitchToZenMode.boxed_clone(), cx),
    }
}
