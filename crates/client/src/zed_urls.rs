//! Contains helper functions for constructing URLs to ZZZ documentation.
//!
//! Links are intentionally offline-safe; callers can provide their own remote
//! destinations when explicitly configured.

use gpui::App;

/// Returns the URL to the project's terms.
pub fn terms_of_service(cx: &App) -> String {
    let _ = cx;
    "about:blank".to_owned()
}

/// Returns the URL to local AI privacy and security docs.
pub fn ai_privacy_and_security(cx: &App) -> String {
    let _ = cx;
    "about:blank".to_owned()
}

/// Returns the URL to edit prediction documentation.
pub fn edit_prediction_docs(cx: &App) -> String {
    let _ = cx;
    "about:blank".to_owned()
}

/// Returns the URL to the ACP registry blog post.
pub fn acp_registry_blog(cx: &App) -> String {
    let _ = cx;
    "about:blank".to_owned()
}

/// Returns the URL to the Parallel Agents blog post.
pub fn parallel_agents_blog(cx: &App) -> String {
    let _ = cx;
    "about:blank".to_owned()
}
