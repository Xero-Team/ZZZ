//! Contains helper functions for constructing URLs to ZZZ documentation.
//!
//! Links are intentionally offline-safe; callers can provide their own remote
//! destinations when explicitly configured.

use gpui::App;

/// Returns the URL to the ACP registry blog post.
pub fn acp_registry_blog(cx: &App) -> String {
    let _ = cx;
    "about:blank".to_owned()
}
