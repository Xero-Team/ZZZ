use std::mem;

use anyhow::{Result, anyhow, bail};
use futures::{AsyncBufReadExt, AsyncReadExt, StreamExt, io::BufReader, stream::BoxStream};
use http_client::{
    AsyncBody, CustomHeaders, HttpClient, Method, Request as HttpRequest, RequestBuilderExt,
};
pub use language_model_core::ModelMode as GoogleModelMode;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
pub mod completion;

pub const API_URL: &str = "https://generativelanguage.googleapis.com";

pub async fn stream_generate_content(
    client: &dyn HttpClient,
    api_url: &str,
    api_key: &str,
    mut request: GenerateContentRequest,
    extra_headers: &CustomHeaders,
) -> Result<BoxStream<'static, Result<GenerateContentResponse>>> {
    let api_key = api_key.trim();
    validate_generate_content_request(&request)?;

    // The `model` field is emptied as it is provided as a path parameter.
    let model_id = mem::take(&mut request.model.model_id);

    let uri =
        format!("{api_url}/v1beta/models/{model_id}:streamGenerateContent?alt=sse&key={api_key}",);

    let request = HttpRequest::builder()
        .method(Method::POST)
        .uri(uri)
        .header("Content-Type", "application/json")
        .extra_headers(extra_headers)
        .body(AsyncBody::from(serde_json::to_string(&request)?))?;
    let mut response = client.send(request).await?;
    if response.status().is_success() {
        let reader = BufReader::new(response.into_body());
        Ok(reader
            .lines()
            .filter_map(|line| async move {
                match line {
                    Ok(line) => {
                        if let Some(line) = line.strip_prefix("data: ") {
                            // Some proxies and gateways append `data: [DONE]` as a
                            // stream-termination sentinel (an OpenAI convention).
                            // Google's streaming spec does not include this sentinel,
                            // but we tolerate it here for the same reason as the
                            // anthropic crate: proxies that inject it would otherwise
                            // cause a spurious JSON deserialization error.
                            if line.trim() == "[DONE]" {
                                return None;
                            }
                            match serde_json::from_str(line) {
                                Ok(response) => Some(Ok(response)),
                                Err(error) => Some(Err(anyhow!(format!(
                                    "Error parsing JSON: {error:?}\n{line:?}"
                                )))),
                            }
                        } else {
                            None
                        }
                    }
                    Err(error) => Some(Err(anyhow!(error))),
                }
            })
            .boxed())
    } else {
        let mut text = String::new();
        response.body_mut().read_to_string(&mut text).await?;
        Err(anyhow!(
            "error during streamGenerateContent, status code: {:?}, body: {}",
            response.status(),
            text
        ))
    }
}

pub fn validate_generate_content_request(request: &GenerateContentRequest) -> Result<()> {
    if request.model.is_empty() {
        bail!("Model must be specified");
    }

    if request.contents.is_empty() {
        bail!("Request must contain at least one content item");
    }

    if let Some(user_content) = request
        .contents
        .iter()
        .find(|content| content.role == Role::User)
        && user_content.parts.is_empty()
    {
        bail!("User content must contain at least one part");
    }

    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Task {
    #[serde(rename = "generateContent")]
    GenerateContent,
    #[serde(rename = "streamGenerateContent")]
    StreamGenerateContent,
    #[serde(rename = "embedContent")]
    EmbedContent,
    #[serde(rename = "batchEmbedContents")]
    BatchEmbedContents,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentRequest {
    #[serde(default, skip_serializing_if = "ModelName::is_empty")]
    pub model: ModelName,
    pub contents: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<SystemInstruction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_config: Option<GenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_settings: Option<Vec<SafetySetting>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_config: Option<ToolConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates: Option<Vec<GenerateContentCandidate>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_feedback: Option<PromptFeedback>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_metadata: Option<UsageMetadata>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentCandidate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
    pub content: Content,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_ratings: Option<Vec<SafetyRating>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citation_metadata: Option<CitationMetadata>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    #[serde(default)]
    pub parts: Vec<Part>,
    pub role: Role,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInstruction {
    pub parts: Vec<Part>,
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    User,
    Model,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Part {
    TextPart(TextPart),
    InlineDataPart(InlineDataPart),
    FunctionCallPart(FunctionCallPart),
    FunctionResponsePart(FunctionResponsePart),
    ThoughtPart(ThoughtPart),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextPart {
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineDataPart {
    pub inline_data: GenerativeContentBlob,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerativeContentBlob {
    pub mime_type: String,
    pub data: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionCallPart {
    pub function_call: FunctionCall,
    /// Thought signature returned by the model for function calls.
    /// Only present on the first function call in parallel call scenarios.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionResponsePart {
    pub function_response: FunctionResponse,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThoughtPart {
    pub thought: bool,
    pub thought_signature: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationMetadata {
    pub citation_sources: Vec<CitationSource>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptFeedback {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_reason: Option<String>,
    pub safety_ratings: Option<Vec<SafetyRating>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_reason_message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_token_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_content_token_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates_token_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_prompt_token_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thoughts_token_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_token_count: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ThinkingLevel {
    Minimal,
    Low,
    Medium,
    High,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_config: Option<ThinkingConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetySetting {
    pub category: HarmCategory,
    pub threshold: HarmBlockThreshold,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum HarmCategory {
    #[serde(rename = "HARM_CATEGORY_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "HARM_CATEGORY_DEROGATORY")]
    Derogatory,
    #[serde(rename = "HARM_CATEGORY_TOXICITY")]
    Toxicity,
    #[serde(rename = "HARM_CATEGORY_VIOLENCE")]
    Violence,
    #[serde(rename = "HARM_CATEGORY_SEXUAL")]
    Sexual,
    #[serde(rename = "HARM_CATEGORY_MEDICAL")]
    Medical,
    #[serde(rename = "HARM_CATEGORY_DANGEROUS")]
    Dangerous,
    #[serde(rename = "HARM_CATEGORY_HARASSMENT")]
    Harassment,
    #[serde(rename = "HARM_CATEGORY_HATE_SPEECH")]
    HateSpeech,
    #[serde(rename = "HARM_CATEGORY_SEXUALLY_EXPLICIT")]
    SexuallyExplicit,
    #[serde(rename = "HARM_CATEGORY_DANGEROUS_CONTENT")]
    DangerousContent,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HarmBlockThreshold {
    #[serde(rename = "HARM_BLOCK_THRESHOLD_UNSPECIFIED")]
    Unspecified,
    BlockLowAndAbove,
    BlockMediumAndAbove,
    BlockOnlyHigh,
    BlockNone,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HarmProbability {
    #[serde(rename = "HARM_PROBABILITY_UNSPECIFIED")]
    Unspecified,
    Negligible,
    Low,
    Medium,
    High,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetyRating {
    pub category: HarmCategory,
    pub probability: HarmProbability,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub args: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionResponse {
    pub name: String,
    pub response: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub function_declarations: Vec<FunctionDeclaration>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolConfig {
    pub function_calling_config: FunctionCallingConfig,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionCallingConfig {
    pub mode: FunctionCallingMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_function_names: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FunctionCallingMode {
    Auto,
    Any,
    None,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionDeclaration {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Default)]
pub struct ModelName {
    pub model_id: String,
}

impl ModelName {
    pub fn is_empty(&self) -> bool {
        self.model_id.is_empty()
    }
}

const MODEL_NAME_PREFIX: &str = "models/";

impl Serialize for ModelName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{MODEL_NAME_PREFIX}{}", &self.model_id))
    }
}

impl<'de> Deserialize<'de> for ModelName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let string = String::deserialize(deserializer)?;
        if let Some(id) = string.strip_prefix(MODEL_NAME_PREFIX) {
            Ok(Self {
                model_id: id.to_owned(),
            })
        } else {
            Err(serde::de::Error::custom(format!(
                "Expected model name to begin with {}, got: {}",
                MODEL_NAME_PREFIX, string
            )))
        }
    }
}

#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Clone, Default, Debug, Deserialize, Serialize, PartialEq, Eq, strum::EnumIter)]
pub enum Model {
    #[serde(
        rename = "gemini-2.5-flash-lite",
        alias = "gemini-2.5-flash-lite-preview-06-17",
        alias = "gemini-2.0-flash-lite-preview"
    )]
    Gemini25FlashLite,
    #[serde(
        rename = "gemini-2.5-flash",
        alias = "gemini-2.0-flash-thinking-exp",
        alias = "gemini-2.5-flash-preview-04-17",
        alias = "gemini-2.5-flash-preview-05-20",
        alias = "gemini-2.5-flash-preview-latest",
        alias = "gemini-2.0-flash"
    )]
    #[default]
    Gemini25Flash,
    #[serde(
        rename = "gemini-2.5-pro",
        alias = "gemini-2.0-pro-exp",
        alias = "gemini-2.5-pro-preview-latest",
        alias = "gemini-2.5-pro-exp-03-25",
        alias = "gemini-2.5-pro-preview-03-25",
        alias = "gemini-2.5-pro-preview-05-06",
        alias = "gemini-2.5-pro-preview-06-05"
    )]
    Gemini25Pro,
    #[serde(rename = "gemini-3.1-flash-lite")]
    Gemini31FlashLite,
    #[serde(rename = "gemini-3.5-flash-lite")]
    Gemini35FlashLite,
    #[serde(rename = "gemini-3-flash-preview")]
    Gemini3Flash,
    #[serde(rename = "gemini-3.5-flash")]
    Gemini35Flash,
    #[serde(rename = "gemini-3.6-flash")]
    Gemini36Flash,
    #[serde(rename = "gemini-3.7-flash")]
    Gemini37Flash,
    #[serde(rename = "gemini-3.1-pro-preview", alias = "gemini-3-pro-preview")]
    Gemini31Pro,
    #[serde(rename = "custom")]
    Custom {
        name: String,
        /// The name displayed in the UI, such as in the agent panel model dropdown menu.
        display_name: Option<String>,
        max_tokens: u64,
        #[serde(default)]
        mode: GoogleModelMode,
    },
}

impl Model {
    pub fn default_fast() -> Self {
        Self::Gemini31FlashLite
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Gemini25FlashLite => "gemini-2.5-flash-lite",
            Self::Gemini25Flash => "gemini-2.5-flash",
            Self::Gemini25Pro => "gemini-2.5-pro",
            Self::Gemini31FlashLite => "gemini-3.1-flash-lite",
            Self::Gemini35FlashLite => "gemini-3.5-flash-lite",
            Self::Gemini3Flash => "gemini-3-flash-preview",
            Self::Gemini35Flash => "gemini-3.5-flash",
            Self::Gemini36Flash => "gemini-3.6-flash",
            Self::Gemini37Flash => "gemini-3.7-flash",
            Self::Gemini31Pro => "gemini-3.1-pro-preview",
            Self::Custom { name, .. } => name,
        }
    }
    pub fn request_id(&self) -> &str {
        match self {
            Self::Gemini25FlashLite => "gemini-2.5-flash-lite",
            Self::Gemini25Flash => "gemini-2.5-flash",
            Self::Gemini25Pro => "gemini-2.5-pro",
            Self::Gemini31FlashLite => "gemini-3.1-flash-lite",
            Self::Gemini35FlashLite => "gemini-3.5-flash-lite",
            Self::Gemini3Flash => "gemini-3-flash-preview",
            Self::Gemini35Flash => "gemini-3.5-flash",
            Self::Gemini36Flash => "gemini-3.6-flash",
            Self::Gemini37Flash => "gemini-3.7-flash",
            Self::Gemini31Pro => "gemini-3.1-pro-preview",
            Self::Custom { name, .. } => name,
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::Gemini25FlashLite => "Gemini 2.5 Flash-Lite",
            Self::Gemini25Flash => "Gemini 2.5 Flash",
            Self::Gemini25Pro => "Gemini 2.5 Pro",
            Self::Gemini31FlashLite => "Gemini 3.1 Flash Lite",
            Self::Gemini35FlashLite => "Gemini 3.5 Flash-Lite",
            Self::Gemini3Flash => "Gemini 3 Flash",
            Self::Gemini35Flash => "Gemini 3.5 Flash",
            Self::Gemini36Flash => "Gemini 3.6 Flash",
            Self::Gemini37Flash => "Gemini 3.7 Flash",
            Self::Gemini31Pro => "Gemini 3.1 Pro",
            Self::Custom {
                name, display_name, ..
            } => display_name.as_ref().unwrap_or(name),
        }
    }

    pub fn max_token_count(&self) -> u64 {
        match self {
            Self::Gemini25FlashLite
            | Self::Gemini25Flash
            | Self::Gemini25Pro
            | Self::Gemini31FlashLite
            | Self::Gemini35FlashLite
            | Self::Gemini3Flash
            | Self::Gemini35Flash
            | Self::Gemini36Flash
            | Self::Gemini37Flash
            | Self::Gemini31Pro => 1_048_576,
            Self::Custom { max_tokens, .. } => *max_tokens,
        }
    }

    pub fn max_output_tokens(&self) -> Option<u64> {
        match self {
            Model::Gemini25FlashLite
            | Model::Gemini25Flash
            | Model::Gemini25Pro
            | Model::Gemini31FlashLite
            | Model::Gemini35FlashLite
            | Model::Gemini3Flash
            | Model::Gemini35Flash
            | Model::Gemini36Flash
            | Model::Gemini37Flash
            | Model::Gemini31Pro => Some(65_536),
            Model::Custom { .. } => None,
        }
    }

    pub fn supports_tools(&self) -> bool {
        true
    }

    pub fn supports_images(&self) -> bool {
        true
    }

    pub fn mode(&self) -> GoogleModelMode {
        match self {
            Self::Gemini25FlashLite | Self::Gemini25Flash | Self::Gemini25Pro => {
                GoogleModelMode::Thinking {
                    // By default these models are set to "auto", so we preserve that behavior
                    // but indicate they are capable of thinking mode
                    budget_tokens: None,
                }
            }
            Self::Gemini3Flash => GoogleModelMode::Default,
            Self::Gemini31FlashLite => GoogleModelMode::Default,
            Self::Gemini35FlashLite => GoogleModelMode::Default,
            Self::Gemini35Flash | Self::Gemini36Flash | Self::Gemini37Flash => {
                GoogleModelMode::Thinking {
                    budget_tokens: None,
                }
            }
            Self::Gemini31Pro => GoogleModelMode::Thinking {
                budget_tokens: None,
            },
            Self::Custom { mode, .. } => *mode,
        }
    }
}

impl std::fmt::Display for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_request() -> GenerateContentRequest {
        GenerateContentRequest {
            model: ModelName {
                model_id: "gemini-2.5-flash".into(),
            },
            contents: vec![Content {
                role: Role::User,
                parts: vec![Part::TextPart(TextPart {
                    text: "Hello".into(),
                })],
            }],
            system_instruction: None,
            generation_config: None,
            safety_settings: None,
            tools: None,
            tool_config: None,
        }
    }

    #[test]
    fn test_gemini_3_6_flash_model_metadata() {
        let model = Model::Gemini36Flash;
        assert_eq!(model.id(), "gemini-3.6-flash");
        assert_eq!(model.request_id(), "gemini-3.6-flash");
        assert_eq!(model.display_name(), "Gemini 3.6 Flash");

        let serialized = serde_json::to_value(&model).unwrap();
        assert_eq!(serialized, json!("gemini-3.6-flash"));
        let deserialized: Model = serde_json::from_value(json!("gemini-3.6-flash")).unwrap();
        assert_eq!(deserialized, Model::Gemini36Flash);
    }

    #[test]
    fn test_gemini_3_7_flash_model_metadata() {
        let model = Model::Gemini37Flash;
        assert_eq!(model.id(), "gemini-3.7-flash");
        assert_eq!(model.request_id(), "gemini-3.7-flash");
        assert_eq!(model.display_name(), "Gemini 3.7 Flash");

        let serialized = serde_json::to_value(&model).unwrap();
        assert_eq!(serialized, json!("gemini-3.7-flash"));
        let deserialized: Model = serde_json::from_value(json!("gemini-3.7-flash")).unwrap();
        assert_eq!(deserialized, Model::Gemini37Flash);
    }

    #[test]
    fn validate_generate_content_request_rejects_missing_required_fields() {
        let mut request = valid_request();
        request.model = ModelName::default();
        let error = validate_generate_content_request(&request).expect_err("missing model");
        assert!(error.to_string().contains("Model must be specified"));

        let mut request = valid_request();
        request.contents.clear();
        let error = validate_generate_content_request(&request).expect_err("missing contents");
        assert!(
            error
                .to_string()
                .contains("Request must contain at least one content item")
        );

        let mut request = valid_request();
        request.contents[0].parts.clear();
        let error = validate_generate_content_request(&request).expect_err("empty user parts");
        assert!(
            error
                .to_string()
                .contains("User content must contain at least one part")
        );
    }

    #[test]
    fn validate_generate_content_request_accepts_non_user_empty_parts() {
        let request = GenerateContentRequest {
            model: ModelName {
                model_id: "gemini-2.5-flash".into(),
            },
            contents: vec![Content {
                role: Role::Model,
                parts: vec![],
            }],
            system_instruction: None,
            generation_config: None,
            safety_settings: None,
            tools: None,
            tool_config: None,
        };

        validate_generate_content_request(&request).expect("model role may have empty parts");
    }

    #[test]
    fn model_name_serialization_requires_models_prefix() {
        let model_name = ModelName {
            model_id: "gemini-2.5-flash".into(),
        };
        let serialized = serde_json::to_string(&model_name).expect("serialize model name");
        assert_eq!(serialized, "\"models/gemini-2.5-flash\"");

        let deserialized: ModelName =
            serde_json::from_str("\"models/gemini-2.5-flash\"").expect("deserialize model name");
        assert_eq!(deserialized.model_id, "gemini-2.5-flash");

        let error = serde_json::from_str::<ModelName>("\"gemini-2.5-flash\"")
            .expect_err("missing prefix must fail");
        assert!(
            error
                .to_string()
                .contains("Expected model name to begin with models/")
        );
    }

    #[test]
    fn model_helpers_cover_built_in_aliases_and_custom_modes() {
        let alias: Model = serde_json::from_str("\"gemini-2.5-flash-preview-latest\"")
            .expect("alias deserializes");
        assert_eq!(alias, Model::Gemini25Flash);

        let default_model = Model::default();
        assert_eq!(default_model, Model::Gemini25Flash);
        assert_eq!(Model::default_fast(), Model::Gemini31FlashLite);
        assert_eq!(default_model.id(), "gemini-2.5-flash");
        assert_eq!(default_model.request_id(), "gemini-2.5-flash");
        assert_eq!(default_model.display_name(), "Gemini 2.5 Flash");
        assert_eq!(default_model.max_token_count(), 1_048_576);
        assert_eq!(default_model.max_output_tokens(), Some(65_536));
        assert!(default_model.supports_tools());
        assert!(default_model.supports_images());
        assert!(matches!(
            default_model.mode(),
            GoogleModelMode::Thinking {
                budget_tokens: None
            }
        ));

        let custom = Model::Custom {
            name: "custom/gemini".into(),
            display_name: None,
            max_tokens: 1234,
            mode: GoogleModelMode::Default,
        };
        assert_eq!(custom.id(), "custom/gemini");
        assert_eq!(custom.request_id(), "custom/gemini");
        assert_eq!(custom.display_name(), "custom/gemini");
        assert_eq!(custom.max_token_count(), 1234);
        assert_eq!(custom.max_output_tokens(), None);
        assert!(matches!(custom.mode(), GoogleModelMode::Default));
        assert_eq!(custom.to_string(), "custom/gemini");
    }

    #[test]
    fn test_function_call_part_with_signature_serializes_correctly() {
        let part = FunctionCallPart {
            function_call: FunctionCall {
                name: "test_function".to_string(),
                args: json!({"arg": "value"}),
                id: None,
            },
            thought_signature: Some("test_signature".to_string()),
        };

        let serialized = serde_json::to_value(&part).unwrap();

        assert_eq!(serialized["functionCall"]["name"], "test_function");
        assert_eq!(serialized["functionCall"]["args"]["arg"], "value");
        assert_eq!(serialized["thoughtSignature"], "test_signature");
    }

    #[test]
    fn test_function_call_part_without_signature_omits_field() {
        let part = FunctionCallPart {
            function_call: FunctionCall {
                name: "test_function".to_string(),
                args: json!({"arg": "value"}),
                id: None,
            },
            thought_signature: None,
        };

        let serialized = serde_json::to_value(&part).unwrap();

        assert_eq!(serialized["functionCall"]["name"], "test_function");
        assert_eq!(serialized["functionCall"]["args"]["arg"], "value");
        // thoughtSignature field should not be present when None
        assert!(serialized.get("thoughtSignature").is_none());
    }

    #[test]
    fn test_function_call_part_deserializes_with_signature() {
        let json = json!({
            "functionCall": {
                "name": "test_function",
                "args": {"arg": "value"}
            },
            "thoughtSignature": "test_signature"
        });

        let part: FunctionCallPart = serde_json::from_value(json).unwrap();

        assert_eq!(part.function_call.name, "test_function");
        assert_eq!(part.thought_signature, Some("test_signature".to_string()));
    }

    #[test]
    fn test_function_call_part_deserializes_without_signature() {
        let json = json!({
            "functionCall": {
                "name": "test_function",
                "args": {"arg": "value"}
            }
        });

        let part: FunctionCallPart = serde_json::from_value(json).unwrap();

        assert_eq!(part.function_call.name, "test_function");
        assert_eq!(part.thought_signature, None);
    }

    #[test]
    fn test_function_call_part_round_trip() {
        let original = FunctionCallPart {
            function_call: FunctionCall {
                name: "test_function".to_string(),
                args: json!({"arg": "value", "nested": {"key": "val"}}),
                id: None,
            },
            thought_signature: Some("round_trip_signature".to_string()),
        };

        let serialized = serde_json::to_value(&original).unwrap();
        let deserialized: FunctionCallPart = serde_json::from_value(serialized).unwrap();

        assert_eq!(deserialized.function_call.name, original.function_call.name);
        assert_eq!(deserialized.function_call.args, original.function_call.args);
        assert_eq!(deserialized.thought_signature, original.thought_signature);
    }

    #[test]
    fn test_function_call_part_with_empty_signature_serializes() {
        let part = FunctionCallPart {
            function_call: FunctionCall {
                name: "test_function".to_string(),
                args: json!({"arg": "value"}),
                id: None,
            },
            thought_signature: Some("".to_string()),
        };

        let serialized = serde_json::to_value(&part).unwrap();

        // Empty string should still be serialized (normalization happens at a higher level)
        assert_eq!(serialized["thoughtSignature"], "");
    }
}
