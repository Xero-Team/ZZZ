use std::sync::Arc;

use anyhow::{Context as _, Result};
use client::{Client, NeedsLlmTokenRefresh, UserStore, global_llm_token};
use cloud_api_client::LlmApiToken;
use cloud_api_types::OrganizationId;
use cloud_llm_client::{WebSearchBody, WebSearchResponse};
use futures::AsyncReadExt as _;
use gpui::{App, AppContext, Context, Entity, Task};
use http_client::{AsyncBody, HttpClient, Method};
use web_search::{WebSearchProvider, WebSearchProviderId};

pub struct CloudWebSearchProvider {
    state: Entity<State>,
}

impl CloudWebSearchProvider {
    pub fn new(client: Arc<Client>, user_store: Entity<UserStore>, cx: &mut App) -> Self {
        let state = cx.new(|cx| State::new(client, user_store, cx));

        Self { state }
    }
}

pub struct State {
    client: Arc<Client>,
    user_store: Entity<UserStore>,
    llm_api_token: LlmApiToken,
}

impl State {
    pub fn new(client: Arc<Client>, user_store: Entity<UserStore>, cx: &mut Context<Self>) -> Self {
        let llm_api_token = global_llm_token(cx);

        Self {
            client,
            user_store,
            llm_api_token,
        }
    }
}

pub const ZED_WEB_SEARCH_PROVIDER_ID: &str = "zed.dev";

impl WebSearchProvider for CloudWebSearchProvider {
    fn id(&self) -> WebSearchProviderId {
        WebSearchProviderId(ZED_WEB_SEARCH_PROVIDER_ID.into())
    }

    fn search(&self, query: String, cx: &mut App) -> Task<Result<WebSearchResponse>> {
        let state = self.state.read(cx);
        let client = state.client.clone();
        let llm_api_token = state.llm_api_token.clone();
        let organization_id = state
            .user_store
            .read(cx)
            .current_organization()
            .map(|organization| organization.id.clone());
        let body = WebSearchBody { query };
        cx.background_spawn(async move {
            perform_web_search(client, llm_api_token, organization_id, body).await
        })
    }
}

async fn perform_web_search(
    client: Arc<Client>,
    llm_api_token: LlmApiToken,
    organization_id: Option<OrganizationId>,
    body: WebSearchBody,
) -> Result<WebSearchResponse> {
    let http_client = &client.http_client();
    let request_body = serde_json::to_string(&body)?;

    perform_web_search_with(
        || {
            let client = client.clone();
            let llm_api_token = llm_api_token.clone();
            let organization_id = organization_id.clone();
            async move {
                client
                    .acquire_llm_token(&llm_api_token, organization_id.clone())
                    .await
            }
        },
        || {
            let client = client.clone();
            let llm_api_token = llm_api_token.clone();
            let organization_id = organization_id.clone();
            async move {
                client
                    .refresh_llm_token(&llm_api_token, organization_id.clone())
                    .await
            }
        },
        |token| {
            let http_client = http_client.clone();
            let request_body = request_body.clone();
            async move {
                let request = http_client::Request::builder()
                    .method(Method::POST)
                    .uri(http_client.build_zed_llm_url("/web_search", &[])?.as_ref())
                    .header("Content-Type", "application/json")
                    .header("Authorization", format!("Bearer {token}"))
                    .body(request_body.into())?;
                http_client
                    .send(request)
                    .await
                    .context("failed to send web search request")
            }
        },
    )
    .await
}

async fn perform_web_search_with<
    AcquireToken,
    AcquireFuture,
    RefreshToken,
    RefreshFuture,
    Send,
    SendFuture,
>(
    mut acquire_token: AcquireToken,
    mut refresh_token: RefreshToken,
    mut send_request: Send,
) -> Result<WebSearchResponse>
where
    AcquireToken: FnMut() -> AcquireFuture,
    AcquireFuture: Future<Output = Result<String>>,
    RefreshToken: FnMut() -> RefreshFuture,
    RefreshFuture: Future<Output = Result<String>>,
    Send: FnMut(String) -> SendFuture,
    SendFuture: Future<Output = Result<http_client::Response<AsyncBody>>>,
{
    const MAX_RETRIES: usize = 3;

    let mut retries_remaining = MAX_RETRIES;
    let mut token = acquire_token().await?;

    loop {
        if retries_remaining == 0 {
            return Err(anyhow::anyhow!(
                "error performing web search, max retries exceeded"
            ));
        }

        let mut response = send_request(token.clone()).await?;

        if response.status().is_success() {
            let mut body = String::new();
            response.body_mut().read_to_string(&mut body).await?;
            return Ok(serde_json::from_str(&body)?);
        } else if response.needs_llm_token_refresh() {
            token = refresh_token().await?;
            retries_remaining -= 1;
        } else {
            // For now we will only retry if the LLM token is expired,
            // not if the request failed for any other reason.
            let mut body = String::new();
            response.body_mut().read_to_string(&mut body).await?;
            anyhow::bail!(
                "error performing web search.\nStatus: {:?}\nBody: {body}",
                response.status(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cloud_llm_client::EXPIRED_LLM_TOKEN_HEADER_NAME;
    use futures::executor::block_on;
    use std::sync::{Arc, Mutex};

    fn response_with_status(status: u16, body: &str) -> http_client::Response<AsyncBody> {
        http_client::Response::builder()
            .status(status)
            .body(body.to_owned().into())
            .expect("response")
    }

    #[test]
    fn web_search_succeeds_with_initial_token() {
        let sent_tokens = Arc::new(Mutex::new(Vec::new()));
        let sent_tokens_for_request = sent_tokens.clone();

        let response = block_on(perform_web_search_with(
            || async { Ok("initial-token".to_string()) },
            || async { Ok("refreshed-token".to_string()) },
            move |token| {
                let sent_tokens = sent_tokens_for_request.clone();
                async move {
                    sent_tokens.lock().expect("tokens").push(token);
                    Ok(response_with_status(
                        200,
                        r#"{"results":[{"title":"Title","url":"https://example.com","text":"Body"}]}"#,
                    ))
                }
            },
        ))
        .expect("web search result");

        assert_eq!(response.results.len(), 1);
        assert_eq!(response.results[0].title, "Title");
        assert_eq!(response.results[0].url, "https://example.com");
        assert_eq!(response.results[0].text, "Body");
        assert_eq!(
            sent_tokens.lock().expect("tokens").as_slice(),
            ["initial-token"]
        );
    }

    #[test]
    fn web_search_refreshes_token_and_retries_once() {
        let sent_tokens = Arc::new(Mutex::new(Vec::new()));
        let sent_tokens_for_request = sent_tokens.clone();
        let refresh_count = Arc::new(Mutex::new(0usize));
        let refresh_count_for_request = refresh_count.clone();
        let refresh_count_for_refresh = refresh_count.clone();

        let response = block_on(perform_web_search_with(
            || async { Ok("expired-token".to_string()) },
            move || {
                let refresh_count = refresh_count_for_refresh.clone();
                async move {
                    *refresh_count.lock().expect("refresh count") += 1;
                    Ok("fresh-token".to_string())
                }
            },
            move |token| {
                let sent_tokens = sent_tokens_for_request.clone();
                let refresh_count = refresh_count_for_request.clone();
                async move {
                    sent_tokens.lock().expect("tokens").push(token.clone());

                    if *refresh_count.lock().expect("refresh count") == 0 {
                        Ok(http_client::Response::builder()
                            .status(401)
                            .header(EXPIRED_LLM_TOKEN_HEADER_NAME, "1")
                            .body("expired".to_owned().into())
                            .expect("response"))
                    } else {
                        Ok(response_with_status(
                            200,
                            r#"{"results":[{"title":"Fresh","url":"https://example.com/fresh","text":"Updated"}]}"#,
                        ))
                    }
                }
            },
        ))
        .expect("web search result");

        assert_eq!(*refresh_count.lock().expect("refresh count"), 1);
        assert_eq!(
            sent_tokens.lock().expect("tokens").as_slice(),
            ["expired-token", "fresh-token"]
        );
        assert_eq!(response.results[0].title, "Fresh");
    }

    #[test]
    fn web_search_returns_non_refreshable_errors_without_retry() {
        let refresh_count = Arc::new(Mutex::new(0usize));
        let refresh_count_for_refresh = refresh_count.clone();

        let error = block_on(perform_web_search_with(
            || async { Ok("initial-token".to_string()) },
            move || {
                let refresh_count = refresh_count_for_refresh.clone();
                async move {
                    *refresh_count.lock().expect("refresh count") += 1;
                    Ok("fresh-token".to_string())
                }
            },
            |_token| async { Ok(response_with_status(500, "server exploded")) },
        ))
        .expect_err("non-refreshable error");

        let message = error.to_string();
        assert!(message.contains("error performing web search."));
        assert!(message.contains("500"));
        assert!(message.contains("server exploded"));
        assert_eq!(*refresh_count.lock().expect("refresh count"), 0);
    }

    #[test]
    fn web_search_stops_after_max_refresh_retries() {
        let refresh_count = Arc::new(Mutex::new(0usize));
        let sent_tokens = Arc::new(Mutex::new(Vec::new()));
        let refresh_count_for_refresh = refresh_count.clone();
        let sent_tokens_for_request = sent_tokens.clone();

        let error = block_on(perform_web_search_with(
            || async { Ok("initial-token".to_string()) },
            move || {
                let refresh_count = refresh_count_for_refresh.clone();
                async move {
                    let mut refresh_count = refresh_count.lock().expect("refresh count");
                    *refresh_count += 1;
                    Ok(format!("refreshed-token-{refresh_count}"))
                }
            },
            move |token| {
                let sent_tokens = sent_tokens_for_request.clone();
                async move {
                    sent_tokens.lock().expect("tokens").push(token);
                    Ok(http_client::Response::builder()
                        .status(401)
                        .header(EXPIRED_LLM_TOKEN_HEADER_NAME, "1")
                        .body("expired".to_owned().into())
                        .expect("response"))
                }
            },
        ))
        .expect_err("max retries exceeded");

        assert!(error.to_string().contains("max retries exceeded"));
        assert_eq!(*refresh_count.lock().expect("refresh count"), 3);
        assert_eq!(sent_tokens.lock().expect("tokens").len(), 3);
    }
}
