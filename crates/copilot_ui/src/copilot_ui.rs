mod sign_in;

use std::sync::Arc;

use copilot::GlobalCopilotAuth;
use gpui::AppContext;
use project::DisableAiSettings;
use settings::SettingsStore;
pub use sign_in::{
    ConfigurationMode, ConfigurationView, CopilotCodeVerification, initiate_sign_in,
    initiate_sign_out, reinstall_and_sign_in,
};
use ui::App;
use workspace::AppState;

pub fn init(app_state: &Arc<AppState>, cx: &mut App) {
    let disable_ai = cx.read_global(|settings: &SettingsStore, _| {
        settings.get::<DisableAiSettings>(None).disable_ai
    });
    if !disable_ai {
        GlobalCopilotAuth::set_global(
            app_state.languages.next_language_server_id(),
            app_state.fs.clone(),
            app_state.node_runtime.clone(),
            cx,
        );
    }
}
