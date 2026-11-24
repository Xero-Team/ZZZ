#[cfg(debug_assertions)]
mod div_inspector;
#[cfg(debug_assertions)]
mod inspector;

use std::sync::Arc;

use workspace::AppState;

#[cfg(debug_assertions)]
pub fn init(app_state: Arc<AppState>, cx: &mut gpui::App) {
    inspector::init(app_state, cx);
    set_toggle_inspector_available(cx, true);
}

#[cfg(not(debug_assertions))]
pub fn init(_app_state: Arc<AppState>, cx: &mut gpui::App) {
    use workspace::notifications::NotifyResultExt as _;

    cx.on_action(|_: &zzz_actions::dev::ToggleInspector, cx| {
        Err::<(), anyhow::Error>(anyhow::anyhow!(
            "dev::ToggleInspector is only available in debug builds"
        ))
        .notify_app_err(cx);
    });

    set_toggle_inspector_available(cx, false);
}

/// Records whether the command palette may offer `dev::ToggleInspector`. A build without an
/// inspector keeps the action registered only to explain why it does nothing.
fn set_toggle_inspector_available(cx: &mut gpui::App, available: bool) {
    let action = std::any::TypeId::of::<zzz_actions::dev::ToggleInspector>();
    command_palette_hooks::CommandPaletteFilter::update_global(cx, |filter, _cx| {
        if available {
            filter.show_action_types(&[action]);
        } else {
            filter.hide_action_types(&[action]);
        }
    });
}
