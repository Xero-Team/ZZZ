use anyhow::{Result, anyhow};
use futures::{AsyncBufReadExt, AsyncReadExt, StreamExt, io::BufReader, stream::BoxStream};
use http_client::{
    AsyncBody, CustomHeaders, HttpClient, HttpRequestExt, Method, Request as HttpRequest,
    RequestBuilderExt,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::convert::TryFrom;
use strum::EnumIter;

pub const MISTRAL_API_URL: &str = "https://api.mistral.ai/v1";

#[derive(Clone, Copy, Serialize, Deserialize, Debug, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

impl TryFrom<String> for Role {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self> {
        match value.as_str() {
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            "system" => Ok(Self::System),
            "tool" => Ok(Self::Tool),
            _ => anyhow::bail!("invalid role '{value}'"),
        }
    }
}

impl From<Role> for String {
    fn from(val: Role) -> Self {
        match val {
            Role::User => "user".to_owned(),
            Role::Assistant => "assistant".to_owned(),
            Role::System => "system".to_owned(),
            Role::Tool => "tool".to_owned(),
        }
    }
}

#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, EnumIter)]
pub enum Model {
    #[serde(rename = "codestral-latest", alias = "codestral-latest")]
    #[default]
    CodestralLatest,

    #[serde(rename = "mistral-large-latest", alias = "mistral-large-latest")]
    MistralLargeLatest,
    #[serde(rename = "mistral-medium-latest", alias = "mistral-medium-latest")]
    MistralMediumLatest,
    #[serde(rename = "mistral-small-latest", alias = "mistral-small-latest")]
    MistralSmallLatest,

    #[serde(rename = "magistral-medium-latest", alias = "magistral-medium-latest")]
    MagistralMediumLatest,

    #[serde(rename = "open-mistral-nemo", alias = "open-mistral-nemo")]
    OpenMistralNemo,

    #[serde(rename = "devstral-medium-latest", alias = "devstral-medium-latest")]
    DevstralMediumLatest,

    #[serde(rename = "ministral-3b-latest", alias = "ministral-3b-latest")]
    Ministral3bLatest,
    #[serde(rename = "ministral-8b-latest", alias = "ministral-8b-latest")]
    Ministral8bLatest,
    #[serde(rename = "ministral-14b-latest", alias = "ministral-14b-latest")]
    Ministral14bLatest,

    #[serde(rename = "custom")]
    Custom {
        name: String,
        /// The name displayed in the UI, such as in the agent panel model dropdown menu.
        display_name: Option<String>,
        max_tokens: u64,
        max_output_tokens: Option<u64>,
        max_completion_tokens: Option<u64>,
        supports_tools: Option<bool>,
        supports_images: Option<bool>,
        supports_thinking: Option<bool>,
    },
}

impl Model {
    pub fn default_fast() -> Self {
        Model::MistralSmallLatest
    }

    pub fn from_id(id: &str) -> Result<Self> {
        match id {
            "codestral-latest" => Ok(Self::CodestralLatest),
            "mistral-large-latest" => Ok(Self::MistralLargeLatest),
            "mistral-medium-latest" => Ok(Self::MistralMediumLatest),
            "mistral-small-latest" => Ok(Self::MistralSmallLatest),
            "magistral-medium-latest" => Ok(Self::MagistralMediumLatest),
            "open-mistral-nemo" => Ok(Self::OpenMistralNemo),
            "devstral-medium-latest" => Ok(Self::DevstralMediumLatest),
            invalid_id => anyhow::bail!("invalid model id '{invalid_id}'"),
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::CodestralLatest => "codestral-latest",
            Self::MistralLargeLatest => "mistral-large-latest",
            Self::MistralMediumLatest => "mistral-medium-latest",
            Self::MistralSmallLatest => "mistral-small-latest",
            Self::MagistralMediumLatest => "magistral-medium-latest",
            Self::OpenMistralNemo => "open-mistral-nemo",
            Self::DevstralMediumLatest => "devstral-medium-latest",
            Self::Ministral3bLatest => "ministral-3b-latest",
            Self::Ministral8bLatest => "ministral-8b-latest",
            Self::Ministral14bLatest => "ministral-14b-latest",
            Self::Custom { name, .. } => name,
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::CodestralLatest => "codestral-latest",
            Self::MistralLargeLatest => "mistral-large-latest",
            Self::MistralMediumLatest => "mistral-medium-latest",
            Self::MistralSmallLatest => "mistral-small-latest",
            Self::MagistralMediumLatest => "magistral-medium-latest",
            Self::OpenMistralNemo => "open-mistral-nemo",
            Self::DevstralMediumLatest => "devstral-medium-latest",
            Self::Ministral3bLatest => "ministral-3b-latest",
            Self::Ministral8bLatest => "ministral-8b-latest",
            Self::Ministral14bLatest => "ministral-14b-latest",
            Self::Custom {
                name, display_name, ..
            } => display_name.as_ref().unwrap_or(name),
        }
    }

    pub fn max_token_count(&self) -> u64 {
        match self {
            Self::CodestralLatest => 128000,
            Self::MistralLargeLatest => 256000,
            Self::MistralMediumLatest => 128000,
            Self::MistralSmallLatest => 256000,
            Self::MagistralMediumLatest => 128000,
            Self::OpenMistralNemo => 128000,
            Self::DevstralMediumLatest => 256000,
            Self::Ministral3bLatest => 256000,
            Self::Ministral8bLatest => 256000,
            Self::Ministral14bLatest => 256000,
            Self::Custom { max_tokens, .. } => *max_tokens,
        }
    }

    pub fn max_output_tokens(&self) -> Option<u64> {
        match self {
            Self::Custom {
                max_output_tokens, ..
            } => *max_output_tokens,
            _ => None,
        }
    }

    pub fn supports_tools(&self) -> bool {
        match self {
            Self::CodestralLatest
            | Self::MistralLargeLatest
            | Self::MistralMediumLatest
            | Self::MistralSmallLatest
            | Self::MagistralMediumLatest
            | Self::OpenMistralNemo
            | Self::DevstralMediumLatest
            | Self::Ministral3bLatest
            | Self::Ministral8bLatest
            | Self::Ministral14bLatest => true,
            Self::Custom { supports_tools, .. } => supports_tools.unwrap_or(false),
        }
    }

    pub fn supports_images(&self) -> bool {
        match self {
            Self::MistralLargeLatest
            | Self::MistralMediumLatest
            | Self::MistralSmallLatest
            | Self::MagistralMediumLatest
            | Self::Ministral3bLatest
            | Self::Ministral8bLatest
            | Self::Ministral14bLatest => true,
            Self::CodestralLatest | Self::OpenMistralNemo | Self::DevstralMediumLatest => false,
            Self::Custom {
                supports_images, ..
            } => supports_images.unwrap_or(false),
        }
    }

    pub fn supports_thinking(&self) -> bool {
        match self {
            Self::MagistralMediumLatest => true,
            Self::Custom {
                supports_thinking, ..
            } => supports_thinking.unwrap_or(false),
            _ => false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub model: String,
    pub messages: Vec<RequestMessage>,
    pub stream: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolDefinition>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StreamOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_tool_calls: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseFormat {
    Text,
    #[serde(rename = "json_object")]
    JsonObject,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolDefinition {
    Function { function: FunctionDefinition },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolChoice {
    Auto,
    Required,
    None,
    Any,
    #[serde(untagged)]
    Function(ToolDefinition),
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum RequestMessage {
    Assistant {
        #[serde(flatten)]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content: Option<MessageContent>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<ToolCall>,
    },
    User {
        #[serde(flatten)]
        content: MessageContent,
    },
    System {
        #[serde(flatten)]
        content: MessageContent,
    },
    Tool {
        content: String,
        tool_call_id: String,
    },
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(untagged)]
pub enum MessageContent {
    #[serde(rename = "content")]
    Plain { content: String },
    #[serde(rename = "content")]
    Multipart { content: Vec<MessagePart> },
}

impl MessageContent {
    pub fn empty() -> Self {
        Self::Plain {
            content: String::new(),
        }
    }

    pub fn push_part(&mut self, part: MessagePart) {
        match self {
            Self::Plain { content } => match part {
                MessagePart::Text { text } => {
                    content.push_str(&text);
                }
                part => {
                    let mut parts = if content.is_empty() {
                        Vec::new()
                    } else {
                        vec![MessagePart::Text {
                            text: content.clone(),
                        }]
                    };
                    parts.push(part);
                    *self = Self::Multipart { content: parts };
                }
            },
            Self::Multipart { content } => {
                content.push(part);
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessagePart {
    Text { text: String },
    ImageUrl { image_url: String },
    Thinking { thinking: Vec<ThinkingPart> },
}

// Backwards-compatibility alias for provider code that refers to ContentPart
pub type ContentPart = MessagePart;

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ThinkingPart {
    Text { text: String },
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct ToolCall {
    pub id: String,
    #[serde(flatten)]
    pub content: ToolCallContent,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ToolCallContent {
    Function { function: FunctionContent },
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct FunctionContent {
    pub name: String,
    pub arguments: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StreamResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<StreamChoice>,
    pub usage: Option<Usage>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StreamChoice {
    pub index: u32,
    pub delta: StreamDelta,
    pub finish_reason: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StreamDelta {
    pub role: Option<Role>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<MessageContentDelta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallChunk>>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(untagged)]
pub enum MessageContentDelta {
    Text(String),
    Parts(Vec<MessagePart>),
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct ToolCallChunk {
    pub index: usize,
    pub id: Option<String>,
    pub function: Option<FunctionChunk>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct FunctionChunk {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

pub async fn stream_completion(
    client: &dyn HttpClient,
    api_url: &str,
    api_key: &str,
    request: Request,
    affinity: Option<String>,
    extra_headers: &CustomHeaders,
) -> Result<BoxStream<'static, Result<StreamResponse>>> {
    let uri = format!("{api_url}/chat/completions");
    let request_builder = HttpRequest::builder()
        .method(Method::POST)
        .uri(uri)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key.trim()))
        .when_some(affinity, |this, affinity| {
            this.header("x-affinity", affinity)
        })
        .extra_headers(extra_headers);

    let request = request_builder.body(AsyncBody::from(serde_json::to_string(&request)?))?;
    let mut response = client.send(request).await?;

    if response.status().is_success() {
        let reader = BufReader::new(response.into_body());
        Ok(reader
            .lines()
            .filter_map(|line| async move {
                match line {
                    Ok(line) => {
                        let line = line.strip_prefix("data: ")?;
                        if line == "[DONE]" {
                            None
                        } else {
                            match serde_json::from_str(line) {
                                Ok(response) => Some(Ok(response)),
                                Err(error) => Some(Err(anyhow!(error))),
                            }
                        }
                    }
                    Err(error) => Some(Err(anyhow!(error))),
                }
            })
            .boxed())
    } else {
        let mut body = String::new();
        response.body_mut().read_to_string(&mut body).await?;
        anyhow::bail!(
            "Failed to connect to Mistral API: {} {}",
            response.status(),
            body,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_round_trip_and_invalid_value_are_stable() {
        assert_eq!(
            Role::try_from("user".to_string()).expect("user"),
            Role::User
        );
        assert_eq!(
            Role::try_from("assistant".to_string()).expect("assistant"),
            Role::Assistant
        );
        assert_eq!(String::from(Role::System), "system");
        assert_eq!(String::from(Role::Tool), "tool");

        let error = Role::try_from("invalid".to_string()).expect_err("invalid role");
        assert!(error.to_string().contains("invalid role 'invalid'"));
    }

    #[test]
    fn model_helpers_cover_built_in_and_custom_variants() {
        assert_eq!(Model::default(), Model::CodestralLatest);
        assert_eq!(Model::default_fast(), Model::MistralSmallLatest);
        assert_eq!(
            Model::from_id("mistral-large-latest").expect("large"),
            Model::MistralLargeLatest
        );
        assert_eq!(
            Model::from_id("magistral-medium-latest").expect("magistral"),
            Model::MagistralMediumLatest
        );

        let built_in = Model::MistralSmallLatest;
        assert_eq!(built_in.id(), "mistral-small-latest");
        assert_eq!(built_in.display_name(), "mistral-small-latest");
        assert_eq!(built_in.max_token_count(), 256000);
        assert_eq!(built_in.max_output_tokens(), None);
        assert!(built_in.supports_tools());
        assert!(built_in.supports_images());
        assert!(!built_in.supports_thinking());

        let thinking = Model::MagistralMediumLatest;
        assert!(thinking.supports_tools());
        assert!(thinking.supports_images());
        assert!(thinking.supports_thinking());

        let custom = Model::Custom {
            name: "mistral/custom".into(),
            display_name: None,
            max_tokens: 1234,
            max_output_tokens: Some(111),
            max_completion_tokens: Some(77),
            supports_tools: Some(false),
            supports_images: Some(true),
            supports_thinking: Some(true),
        };
        assert_eq!(custom.id(), "mistral/custom");
        assert_eq!(custom.display_name(), "mistral/custom");
        assert_eq!(custom.max_token_count(), 1234);
        assert_eq!(custom.max_output_tokens(), Some(111));
        assert!(!custom.supports_tools());
        assert!(custom.supports_images());
        assert!(custom.supports_thinking());
    }

    #[test]
    fn invalid_model_id_returns_actionable_error() {
        let error = Model::from_id("unknown-model").expect_err("unknown model");
        assert!(
            error
                .to_string()
                .contains("invalid model id 'unknown-model'")
        );
    }

    #[test]
    fn message_content_push_part_preserves_plain_text_until_non_text_part() {
        let mut content = MessageContent::empty();
        content.push_part(MessagePart::Text {
            text: "hello".into(),
        });
        content.push_part(MessagePart::Text {
            text: " world".into(),
        });
        assert_eq!(
            content,
            MessageContent::Plain {
                content: "hello world".into()
            }
        );

        content.push_part(MessagePart::ImageUrl {
            image_url: "https://example.com/image.png".into(),
        });
        assert!(matches!(content, MessageContent::Multipart { .. }));
    }

    #[test]
    fn request_message_and_content_delta_round_trip() {
        let message = RequestMessage::Assistant {
            content: Some(MessageContent::Multipart {
                content: vec![
                    MessagePart::Text {
                        text: "hello".into(),
                    },
                    MessagePart::Thinking {
                        thinking: vec![ThinkingPart::Text {
                            text: "reason".into(),
                        }],
                    },
                ],
            }),
            tool_calls: vec![ToolCall {
                id: "tool-1".into(),
                content: ToolCallContent::Function {
                    function: FunctionContent {
                        name: "lookup".into(),
                        arguments: "{\"query\":\"rust\"}".into(),
                    },
                },
            }],
        };
        let serialized = serde_json::to_value(&message).expect("serialize");
        let deserialized: RequestMessage = serde_json::from_value(serialized).expect("deserialize");
        assert_eq!(deserialized, message);

        let text_delta: MessageContentDelta =
            serde_json::from_value(serde_json::json!("chunk")).expect("text delta");
        assert_eq!(text_delta, MessageContentDelta::Text("chunk".into()));

        let parts_delta: MessageContentDelta = serde_json::from_value(serde_json::json!([
            { "type": "text", "text": "chunk" }
        ]))
        .expect("parts delta");
        assert_eq!(
            parts_delta,
            MessageContentDelta::Parts(vec![MessagePart::Text {
                text: "chunk".into()
            }])
        );
    }
}
