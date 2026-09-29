//! The Telegram panel: a bottom-dock panel backed by the `telegram` engine.
//!
//! Nothing connects at application start. The engine lazily starts the first
//! time the panel becomes active or the user presses Sign in, and it never
//! sends telemetry.

mod telegram_forward_picker;
mod telegram_panel;
mod telegram_panel_settings;

pub use telegram_panel::{
    Back, GlobalTelegramEngine, NextChat, PreviousChat, Refresh, Send, TelegramPanel, ToggleFocus,
};
pub use telegram_panel_settings::{TelegramPanelSettings, TelegramSettings};

use std::sync::Arc;

use gpui::App;
use telegram::EngineHandle;

/// Registers the Telegram engine global and the panel actions.
pub fn init(cx: &mut App) {
    let engine = Arc::new(EngineHandle::spawn(telegram_panel::engine_config(cx)));
    cx.set_global(GlobalTelegramEngine(engine));
    cx.observe_new(|workspace: &mut workspace::Workspace, _, cx| {
        telegram_panel::register(workspace, cx);
    })
    .detach();
}
