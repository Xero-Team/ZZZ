use std::rc::Rc;
use std::sync::Arc;
use std::{borrow::Cow, cell::RefCell};

use agent_client_protocol::schema as acp;
use anyhow::{Context as _, Result, bail};
use futures::{AsyncReadExt as _, FutureExt as _};
use gpui::{App, AppContext as _, Task};
use html_to_markdown::{TagHandler, convert_html_to_markdown, markdown};
use http_client::{AsyncBody, HttpClientWithUrl};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ui::SharedString;
use util::markdown::{MarkdownEscaped, MarkdownInlineCode};

use crate::{AgentTool, ToolCallEventStream, ToolInput};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
enum ContentType {
    Html,
    Plaintext,
    Json,
}

/// The maximum number of HTTP redirects the fetch tool will follow. Each hop
/// receives its own tool-permission check before being requested.
const MAX_REDIRECTS: usize = 20;

/// The outcome of a single request with automatic redirect handling disabled.
enum FetchStep {
    Redirect(String),
    Complete(String),
}

/// Prepends `https://` when the URL has no explicit HTTP(S) scheme.
fn normalize_url(url: &str) -> Cow<'_, str> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        Cow::Owned(format!("https://{url}"))
    } else {
        Cow::Borrowed(url)
    }
}

/// Fetches a URL and returns the content as Markdown.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FetchToolInput {
    /// The URL to fetch.
    url: String,
}

pub struct FetchTool {
    http_client: Arc<HttpClientWithUrl>,
}

impl FetchTool {
    pub fn new(http_client: Arc<HttpClientWithUrl>) -> Self {
        Self { http_client }
    }

    /// Performs a single HTTP GET without following redirects. Redirect targets
    /// are returned to the caller so their URL can be separately authorized.
    async fn fetch_step(http_client: Arc<HttpClientWithUrl>, url: &str) -> Result<FetchStep> {
        let normalized = normalize_url(url);

        let mut response = http_client
            .get(&normalized, AsyncBody::default(), false)
            .await?;

        let status = response.status();
        if status.is_redirection() {
            let location = response
                .headers()
                .get("location")
                .context("redirect response is missing a Location header")?
                .to_str()
                .context("redirect response has an invalid Location header")?;
            let target = url::Url::parse(&normalized)
                .with_context(|| format!("could not parse URL {normalized:?}"))?
                .join(location)
                .with_context(|| format!("invalid redirect target {location:?}"))?;
            anyhow::ensure!(
                matches!(target.scheme(), "http" | "https"),
                "refusing to follow redirect to non-HTTP(S) URL {target}"
            );
            return Ok(FetchStep::Redirect(target.to_string()));
        }

        let mut body = Vec::new();
        response
            .body_mut()
            .read_to_end(&mut body)
            .await
            .context("error reading response body")?;

        if status.is_client_error() {
            let text = String::from_utf8_lossy(body.as_slice());
            bail!("status error {}, response: {text:?}", status.as_u16());
        }

        let Some(content_type) = response.headers().get("content-type") else {
            bail!("missing Content-Type header");
        };
        let content_type = content_type
            .to_str()
            .context("invalid Content-Type header")?;

        let content_type = if content_type.starts_with("text/plain") {
            ContentType::Plaintext
        } else if content_type.starts_with("application/json") {
            ContentType::Json
        } else {
            ContentType::Html
        };

        let text = match content_type {
            ContentType::Html => {
                let mut handlers: Vec<TagHandler> = vec![
                    Rc::new(RefCell::new(markdown::WebpageChromeRemover)),
                    Rc::new(RefCell::new(markdown::ParagraphHandler)),
                    Rc::new(RefCell::new(markdown::HeadingHandler)),
                    Rc::new(RefCell::new(markdown::ListHandler)),
                    Rc::new(RefCell::new(markdown::TableHandler::new())),
                    Rc::new(RefCell::new(markdown::StyledTextHandler)),
                ];
                if normalized.contains("wikipedia.org") {
                    use html_to_markdown::structure::wikipedia;

                    handlers.push(Rc::new(RefCell::new(wikipedia::WikipediaChromeRemover)));
                    handlers.push(Rc::new(RefCell::new(wikipedia::WikipediaInfoboxHandler)));
                    handlers.push(Rc::new(
                        RefCell::new(wikipedia::WikipediaCodeHandler::new()),
                    ));
                } else {
                    handlers.push(Rc::new(RefCell::new(markdown::CodeHandler)));
                }

                convert_html_to_markdown(&body[..], &mut handlers)?
            }
            ContentType::Plaintext => std::str::from_utf8(&body)?.to_owned(),
            ContentType::Json => {
                let json: serde_json::Value = serde_json::from_slice(&body)?;

                format!("```json\n{}\n```", serde_json::to_string_pretty(&json)?)
            }
        };

        Ok(FetchStep::Complete(text))
    }
}

impl AgentTool for FetchTool {
    type Input = FetchToolInput;
    type Output = String;

    const NAME: &'static str = "fetch";

    fn kind() -> acp::ToolKind {
        acp::ToolKind::Fetch
    }

    fn initial_title(
        &self,
        input: Result<Self::Input, serde_json::Value>,
        _cx: &mut App,
    ) -> SharedString {
        match input {
            Ok(input) => format!("Fetch {}", MarkdownEscaped(&input.url)).into(),
            Err(_) => "Fetch URL".into(),
        }
    }

    fn run(
        self: Arc<Self>,
        input: ToolInput<Self::Input>,
        event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<Self::Output, Self::Output>> {
        let http_client = self.http_client.clone();
        cx.spawn(async move |cx| {
            let input: FetchToolInput = input.recv().await.map_err(|e| e.to_string())?;

            let mut current_url = input.url;
            let mut redirects = 0;
            let text = loop {
                let authorize = cx.update(|cx| {
                    let context =
                        crate::ToolPermissionContext::new(Self::NAME, vec![current_url.clone()]);

                    event_stream.authorize(
                        format!("Fetch {}", MarkdownInlineCode(&current_url)),
                        context,
                        cx,
                    )
                });
                futures::select! {
                    result = authorize.fuse() => result.map_err(|e| e.to_string())?,
                    _ = event_stream.cancelled_by_user().fuse() => {
                        return Err("Fetch cancelled by user".to_owned());
                    }
                }

                let fetch_task = cx.background_spawn({
                    let http_client = http_client.clone();
                    let url = current_url.clone();
                    async move { Self::fetch_step(http_client, &url).await }
                });
                let step = futures::select! {
                    result = fetch_task.fuse() => result.map_err(|e| e.to_string())?,
                    _ = event_stream.cancelled_by_user().fuse() => {
                        return Err("Fetch cancelled by user".to_owned());
                    }
                };

                match step {
                    FetchStep::Complete(text) => break text,
                    FetchStep::Redirect(target) => {
                        redirects += 1;
                        if redirects > MAX_REDIRECTS {
                            return Err(format!(
                                "exceeded the maximum of {MAX_REDIRECTS} redirects"
                            ));
                        }
                        current_url = target;
                    }
                }
            };
            if text.trim().is_empty() {
                return Err("no textual content found".to_owned());
            }
            Ok(text)
        })
    }
}
