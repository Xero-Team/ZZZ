use anyhow::Result;
use serde_json::Value;

use crate::migrations::migrate_settings;

/// Agent settings that selected a model for a native, in-tree provider.
const RETIRED_AGENT_KEYS: &[&str] = &[
    "default_model",
    "subagent_model",
    "inline_assistant_model",
    "inline_assistant_use_streaming_tools",
    "commit_message_model",
    "commit_message_instructions",
    "thread_summary_model",
    "inline_alternatives",
    "favorite_models",
    "model_parameters",
];

/// Strips settings that configured in-tree model providers, the inline
/// assistant, and edit prediction. Those surfaces were removed, so the keys
/// would otherwise fail schema validation on load.
pub fn remove_retired_ai_provider_settings(value: &mut Value) -> Result<()> {
    migrate_settings(value, &mut migrate_one)
}

fn migrate_one(object: &mut serde_json::Map<String, Value>) -> Result<()> {
    object.remove("language_models");
    object.remove("edit_predictions");

    if let Some(languages) = object.get_mut("languages").and_then(|v| v.as_object_mut()) {
        languages.remove("edit_predictions");
    }

    if let Some(agent) = object.get_mut("agent").and_then(|v| v.as_object_mut()) {
        clean_agent(agent);
    }

    Ok(())
}

fn clean_agent(agent: &mut serde_json::Map<String, Value>) {
    for key in RETIRED_AGENT_KEYS {
        agent.remove(*key);
    }

    if let Some(profiles) = agent.get_mut("profiles").and_then(|v| v.as_object_mut()) {
        for profile in profiles.values_mut() {
            if let Some(profile) = profile.as_object_mut() {
                profile.remove("default_model");
            }
        }
    }
}
