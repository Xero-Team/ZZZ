use anyhow::Result;
use serde::{Deserialize, Serialize};
use strum::EnumIter;

pub const XAI_API_URL: &str = "https://api.x.ai/v1";

#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, EnumIter)]
pub enum Model {
    #[serde(rename = "grok-4.3", alias = "grok-4.3-latest")]
    Grok43,
    #[serde(rename = "grok-4.5", alias = "grok-4.5-latest")]
    Grok45,
    #[serde(rename = "grok-4.6", alias = "grok-4.6-latest")]
    Grok46,
    #[default]
    #[serde(rename = "grok-4.7", alias = "grok-4.7-latest")]
    Grok47,
    #[serde(rename = "grok-4.20-0309-reasoning")]
    Grok420Reasoning,
    #[serde(rename = "grok-4.20-0309-non-reasoning")]
    Grok420NonReasoning,
    #[serde(rename = "custom")]
    Custom {
        name: String,
        /// The name displayed in the UI, such as in the agent panel model dropdown menu.
        display_name: Option<String>,
        max_tokens: u64,
        max_output_tokens: Option<u64>,
        max_completion_tokens: Option<u64>,
        supports_images: Option<bool>,
        supports_tools: Option<bool>,
        parallel_tool_calls: Option<bool>,
    },
}

impl Model {
    pub fn default_fast() -> Self {
        Self::Grok43
    }

    pub fn from_id(id: &str) -> Result<Self> {
        match id {
            "grok-4.3" => Ok(Self::Grok43),
            "grok-4.5" => Ok(Self::Grok45),
            "grok-4.6" => Ok(Self::Grok46),
            "grok-4.7" => Ok(Self::Grok47),
            "grok-4.20-0309-reasoning" => Ok(Self::Grok420Reasoning),
            "grok-4.20-0309-non-reasoning" => Ok(Self::Grok420NonReasoning),
            _ => anyhow::bail!("invalid model id '{id}'"),
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Grok43 => "grok-4.3",
            Self::Grok45 => "grok-4.5",
            Self::Grok46 => "grok-4.6",
            Self::Grok47 => "grok-4.7",
            Self::Grok420Reasoning => "grok-4.20-0309-reasoning",
            Self::Grok420NonReasoning => "grok-4.20-0309-non-reasoning",
            Self::Custom { name, .. } => name,
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::Grok43 => "Grok 4.3",
            Self::Grok45 => "Grok 4.5",
            Self::Grok46 => "Grok 4.6",
            Self::Grok47 => "Grok 4.7",
            Self::Grok420Reasoning => "Grok 4.20 Reasoning",
            Self::Grok420NonReasoning => "Grok 4.20 (Non-Reasoning)",
            Self::Custom {
                name, display_name, ..
            } => display_name.as_ref().unwrap_or(name),
        }
    }

    pub fn max_token_count(&self) -> u64 {
        match self {
            Self::Grok43 => 1_000_000,
            Self::Grok45 | Self::Grok46 | Self::Grok47 => 500_000,
            Self::Grok420Reasoning | Self::Grok420NonReasoning => 2_000_000,
            Self::Custom { max_tokens, .. } => *max_tokens,
        }
    }

    pub fn max_output_tokens(&self) -> Option<u64> {
        match self {
            Self::Grok43 | Self::Grok420Reasoning | Self::Grok420NonReasoning => Some(64_000),
            Self::Grok45 | Self::Grok46 | Self::Grok47 => None,
            Self::Custom {
                max_output_tokens, ..
            } => *max_output_tokens,
        }
    }

    pub fn supports_parallel_tool_calls(&self) -> bool {
        match self {
            Self::Grok43
            | Self::Grok45
            | Self::Grok46
            | Self::Grok47
            | Self::Grok420Reasoning
            | Self::Grok420NonReasoning => true,
            Self::Custom {
                parallel_tool_calls: Some(support),
                ..
            } => *support,
            Model::Custom { .. } => false,
        }
    }

    pub fn requires_json_schema_subset(&self) -> bool {
        match self {
            Self::Grok43
            | Self::Grok45
            | Self::Grok46
            | Self::Grok47
            | Self::Grok420Reasoning
            | Self::Grok420NonReasoning => true,
            Self::Custom { .. } => false,
        }
    }

    pub fn supports_prompt_cache_key(&self) -> bool {
        false
    }

    pub fn supports_tool(&self) -> bool {
        match self {
            Self::Grok43
            | Self::Grok45
            | Self::Grok46
            | Self::Grok47
            | Self::Grok420Reasoning
            | Self::Grok420NonReasoning => true,
            Self::Custom {
                supports_tools: Some(support),
                ..
            } => *support,
            Model::Custom { .. } => false,
        }
    }

    pub fn supports_images(&self) -> bool {
        match self {
            Self::Grok43
            | Self::Grok45
            | Self::Grok46
            | Self::Grok47
            | Self::Grok420Reasoning
            | Self::Grok420NonReasoning => true,
            Self::Custom {
                supports_images: Some(support),
                ..
            } => *support,
            Self::Custom { .. } => false,
        }
    }

    pub fn supports_reasoning_effort(&self) -> bool {
        match self {
            Self::Grok43 | Self::Grok45 | Self::Grok46 | Self::Grok47 => true,
            Self::Grok420Reasoning | Self::Grok420NonReasoning | Self::Custom { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_models_report_expected_metadata() {
        let model = Model::default_fast();
        assert_eq!(model, Model::Grok43);
        assert_eq!(model.id(), "grok-4.3");
        assert_eq!(model.display_name(), "Grok 4.3");
        assert_eq!(model.max_token_count(), 1_000_000);
        assert_eq!(model.max_output_tokens(), Some(64_000));
        assert!(model.supports_parallel_tool_calls());
        assert!(model.requires_json_schema_subset());
        assert!(model.supports_tool());
        assert!(model.supports_images());
        assert!(model.supports_reasoning_effort());
        assert!(!model.supports_prompt_cache_key());
    }

    #[test]
    fn from_id_accepts_known_models_and_rejects_unknown_ones() {
        assert_eq!(Model::from_id("grok-4.3").expect("grok-4.3"), Model::Grok43);
        assert_eq!(
            Model::from_id("grok-4.20-0309-reasoning").expect("reasoning"),
            Model::Grok420Reasoning
        );
        assert_eq!(
            Model::from_id("grok-4.20-0309-non-reasoning").expect("non-reasoning"),
            Model::Grok420NonReasoning
        );

        let error = Model::from_id("unknown-model").expect_err("unknown model must fail");
        assert!(
            error
                .to_string()
                .contains("invalid model id 'unknown-model'")
        );
    }

    #[test]
    fn custom_model_uses_explicit_capabilities_and_name_fallbacks() {
        let explicit = Model::Custom {
            name: "acme/custom".into(),
            display_name: Some("Acme Custom".into()),
            max_tokens: 123_456,
            max_output_tokens: Some(4_096),
            max_completion_tokens: Some(2_048),
            supports_images: Some(true),
            supports_tools: Some(true),
            parallel_tool_calls: Some(true),
        };
        assert_eq!(explicit.id(), "acme/custom");
        assert_eq!(explicit.display_name(), "Acme Custom");
        assert_eq!(explicit.max_token_count(), 123_456);
        assert_eq!(explicit.max_output_tokens(), Some(4_096));
        assert!(explicit.supports_parallel_tool_calls());
        assert!(explicit.supports_tool());
        assert!(explicit.supports_images());
        assert!(!explicit.requires_json_schema_subset());
        assert!(!explicit.supports_reasoning_effort());

        let fallback = Model::Custom {
            name: "acme/fallback".into(),
            display_name: None,
            max_tokens: 99,
            max_output_tokens: None,
            max_completion_tokens: None,
            supports_images: None,
            supports_tools: None,
            parallel_tool_calls: None,
        };
        assert_eq!(fallback.display_name(), "acme/fallback");
        assert_eq!(fallback.max_output_tokens(), None);
        assert!(!fallback.supports_parallel_tool_calls());
        assert!(!fallback.supports_tool());
        assert!(!fallback.supports_images());
    }

    #[test]
    fn serde_alias_deserializes_latest_grok_alias() {
        let model: Model = serde_json::from_str("\"grok-4.3-latest\"").expect("deserialize");
        assert_eq!(model, Model::Grok43);
    }
}
