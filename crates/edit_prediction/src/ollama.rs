use anyhow::{Context as _, Result};
use futures::AsyncReadExt as _;
use gpui::{
    SharedString,
    http_client::{self, HttpClient},
};
use language::language_settings::OpenAiCompatibleEditPredictionSettings;
use ollama::get_models;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize)]
pub(crate) struct OllamaGenerateRequest {
    model: String,
    prompt: String,
    raw: bool,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaGenerateOptions>,
}

#[derive(Debug, Serialize)]
pub(crate) struct OllamaGenerateOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OllamaGenerateResponse {
    pub created_at: String,
    pub response: String,
}

/// Fetches models directly from the configured Ollama server and keeps only
/// models with a known FIM prompt format.
pub async fn fetch_models_from_server(
    http_client: Arc<dyn HttpClient>,
    api_url: &str,
) -> Result<Vec<SharedString>> {
    let mut models: Vec<SharedString> = get_models(
        http_client.as_ref(),
        api_url,
        None,
        &Default::default(),
    )
    .await?
    .into_iter()
    .filter(|model| crate::fim::infer_prompt_format(&model.name).is_some())
    .map(|model| SharedString::new(model.name))
    .collect();
    models.sort();
    Ok(models)
}

pub(crate) async fn make_request(
    settings: OpenAiCompatibleEditPredictionSettings,
    prompt: String,
    stop_tokens: Vec<String>,
    http_client: Arc<dyn HttpClient>,
) -> Result<OllamaGenerateResponse> {
    let request = OllamaGenerateRequest {
        model: settings.model.clone(),
        prompt,
        raw: true,
        stream: false,
        options: Some(OllamaGenerateOptions {
            num_predict: Some(settings.max_output_tokens),
            temperature: Some(0.2),
            stop: Some(stop_tokens),
        }),
    };

    let request_body = serde_json::to_string(&request)?;
    let http_request = http_client::Request::builder()
        .method(http_client::Method::POST)
        .uri(format!("{}/api/generate", settings.api_url))
        .header("Content-Type", "application/json")
        .body(http_client::AsyncBody::from(request_body))?;

    let mut response = http_client.send(http_request).await?;
    let status = response.status();

    log::debug!("Ollama: Response status: {}", status);

    if !status.is_success() {
        let mut body = String::new();
        response.body_mut().read_to_string(&mut body).await?;
        return Err(anyhow::anyhow!("Ollama API error: {} - {}", status, body));
    }

    let mut body = String::new();
    response.body_mut().read_to_string(&mut body).await?;

    let ollama_response: OllamaGenerateResponse =
        serde_json::from_str(&body).context("Failed to parse Ollama response")?;
    Ok(ollama_response)
}
