use std::sync::Arc;

use anyhow::{Result, anyhow};
pub use cloud_api_types::*;
use futures::AsyncReadExt as _;
use http_client::http::request;
use http_client::{AsyncBody, HttpClientWithUrl, Method, Request, StatusCode};
use parking_lot::RwLock;
use thiserror::Error;

struct Credentials {
    user_id: u32,
    access_token: String,
}

#[derive(Debug, Error)]
pub enum ClientApiError {
    /// 401 — credentials are invalid or expired.
    #[error("Unauthorized")]
    Unauthorized,
    /// No credentials have been set on the client.
    #[error("not signed in")]
    NotSignedIn,
    /// Connection-level failure: DNS, TCP, TLS, timeout, etc.
    /// The HTTP request never received a response.
    #[error("connection to {host} failed")]
    ConnectionFailed {
        host: String,
        #[source]
        source: anyhow::Error,
    },
    /// Server returned a non-success HTTP status (other than 401).
    #[error("{host} returned {status}")]
    ServerError {
        host: String,
        status: StatusCode,
        body: String,
    },
    /// Failed to read or parse the response body after a successful HTTP status.
    #[error("invalid response")]
    InvalidResponse(#[source] anyhow::Error),
    /// Failed to build the HTTP request (URL construction, serialization, etc.).
    /// This typically indicates a programming error.
    #[error("failed to build request")]
    RequestBuildFailed(#[source] anyhow::Error),
}

pub struct CloudApiClient {
    credentials: RwLock<Option<Credentials>>,
    http_client: Arc<HttpClientWithUrl>,
}

impl CloudApiClient {
    pub fn new(http_client: Arc<HttpClientWithUrl>) -> Self {
        Self {
            credentials: RwLock::new(None),
            http_client,
        }
    }

    pub fn has_credentials(&self) -> bool {
        self.credentials.read().is_some()
    }

    pub fn set_credentials(&self, user_id: u32, access_token: String) {
        *self.credentials.write() = Some(Credentials {
            user_id,
            access_token,
        });
    }

    pub fn clear_credentials(&self) {
        *self.credentials.write() = None;
    }

    pub async fn validate_credentials(&self, user_id: u32, access_token: &str) -> Result<bool> {
        let request = build_request(
            Request::builder().method(Method::GET).uri(
                self.http_client
                    .build_zed_cloud_url("/client/users/me")?
                    .as_ref(),
            ),
            AsyncBody::default(),
            &Credentials {
                user_id,
                access_token: access_token.into(),
            },
        )?;

        let mut response = self.http_client.send(request).await?;

        if response.status().is_success() {
            Ok(true)
        } else {
            let mut body = String::new();
            response.body_mut().read_to_string(&mut body).await?;
            if response.status() == StatusCode::UNAUTHORIZED {
                Ok(false)
            } else {
                Err(anyhow!(
                    "Failed to get authenticated user.\nStatus: {:?}\nBody: {body}",
                    response.status()
                ))
            }
        }
    }
}

fn build_request(
    req: request::Builder,
    body: impl Into<AsyncBody>,
    credentials: &Credentials,
) -> Result<Request<AsyncBody>> {
    Ok(req
        .header("Content-Type", "application/json")
        .header(
            "Authorization",
            format!("{} {}", credentials.user_id, credentials.access_token),
        )
        .body(body.into())?)
}
