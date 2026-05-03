use crate::Result;
use axum::{Router, routing::post};

pub fn router() -> Router {
    Router::new().route("/telemetry/events", post(post_events))
}

pub async fn post_events() -> Result<()> {
    // Event telemetry has been removed. Keep the endpoint returning OK so old
    // clients do not treat the fork as misconfigured.
    Ok(())
}
