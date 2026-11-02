//! Makes the cached remote-server matrix available to a bundle.

use std::path::PathBuf;

use anyhow::Result;

use crate::tasks::build::model::Profile;
use crate::tasks::bundle::Context;

/// Returns the directory that `ZZZ_EMBED_REMOTE_SERVER_DIR` should name.
///
/// `cargo xtask build` runs the `remote-server` task first and passes the path
/// through the environment. A standalone `cargo xtask bundle` has no planner,
/// so it builds the matrix here when the cache is incomplete.
///
/// A `dev` bundle embeds nothing, matching the embed build script, so there is
/// no point building the matrix for one. `ZZZ_EMBED_REMOTE_SERVERS=1` opts a
/// debug bundle back in.
pub fn ensure(context: &Context) -> Result<Option<PathBuf>> {
    if let Some(directory) = std::env::var_os("ZZZ_EMBED_REMOTE_SERVER_DIR") {
        let directory = PathBuf::from(directory);
        if directory.is_dir() {
            return Ok(Some(directory));
        }
        // The planner hands over a path it computed before the matrix task ran.
        // Falling through is correct (the cache is rebuilt below), but saying so
        // keeps a fingerprint drift from looking like a cache miss.
        eprintln!(
            "warning: ZZZ_EMBED_REMOTE_SERVER_DIR={} is not a directory; rebuilding the matrix",
            directory.display()
        );
    }
    if context.profile == Profile::Dev && !force_embed() {
        return Ok(None);
    }
    crate::tasks::build::remote_server::ensure(
        &context.workspace,
        context.offline,
        Some(context.jobs),
    )
    .map(Some)
}

fn force_embed() -> bool {
    matches!(
        std::env::var("ZZZ_EMBED_REMOTE_SERVERS").ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "YES")
    )
}
