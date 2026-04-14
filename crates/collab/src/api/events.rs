use crate::Result;
use axum::{Router, routing::post};

pub fn router() -> Router {
    Router::new()
        .route("/telemetry/events", post(post_events))
        .route("/telemetry/crashes", post(post_panic))
        .route("/telemetry/panics", post(post_panic))
        .route("/telemetry/hangs", post(post_panic))
}

pub async fn post_panic() -> Result<()> {
    // as of v0.201.x crash/panic reporting is now done via Sentry.
    // The endpoint returns OK to avoid spurious errors for old clients.
    Ok(())
}

pub async fn post_events() -> Result<()> {
    // Event telemetry has been removed. Keep the endpoint returning OK so old
    // clients do not treat the fork as misconfigured.
    Ok(())
}
