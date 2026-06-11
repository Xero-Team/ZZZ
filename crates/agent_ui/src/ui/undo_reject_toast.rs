use action_log::ActionLog;
use gpui::{App, Entity};
use i18n as app_i18n;
use notifications::status_toast::StatusToast;
use ui::prelude::*;
use workspace::Workspace;

fn tr(cx: &App, key: &'static str, fallback: &'static str) -> SharedString {
    app_i18n::tr(cx, key, fallback).into()
}

pub fn show_undo_reject_toast(
    workspace: &mut Workspace,
    action_log: Entity<ActionLog>,
    cx: &mut App,
) {
    let action_log_weak = action_log.downgrade();
    let status_toast = StatusToast::new(
        tr(
            cx,
            "agent_ui.undo_reject.agent_changes_rejected",
            "Agent Changes Rejected",
        ),
        cx,
        move |this, cx| {
            this.icon(
                Icon::new(IconName::Undo)
                    .size(IconSize::Small)
                    .color(Color::Muted),
            )
            .action(
                tr(cx, "agent_ui.undo_reject.undo", "Undo"),
                move |_window, cx| {
                    if let Some(action_log) = action_log_weak.upgrade() {
                        action_log
                            .update(cx, |action_log, cx| action_log.undo_last_reject(cx))
                            .detach();
                    }
                },
            )
            .dismiss_button(true)
        },
    );
    workspace.toggle_status_toast(status_toast, cx);
}
