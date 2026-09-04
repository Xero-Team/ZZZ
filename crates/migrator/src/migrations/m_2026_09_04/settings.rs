use anyhow::Result;
use serde_json::Value;

use crate::migrations::migrate_settings;

pub fn map_rejected_edit_prediction_surfaces_to_absent(value: &mut Value) -> Result<()> {
    migrate_settings(value, &mut |obj| {
        migrate_one(obj);
        Ok(())
    })
}

fn migrate_one(obj: &mut serde_json::Map<String, Value>) {
    if let Some(edit_predictions) = obj.get_mut("edit_predictions")
        && let Some(edit_predictions_obj) = edit_predictions.as_object_mut()
    {
        map_provider_to_none(edit_predictions_obj, "provider");
        edit_predictions_obj.remove("allow_data_collection");
        if let Some(tools) = edit_predictions_obj
            .get_mut("tools")
            .and_then(|value| value.as_object_mut())
        {
            tools.remove("search_web");
        }
    }

    if let Some(features) = obj.get_mut("features")
        && let Some(features_obj) = features.as_object_mut()
    {
        map_provider_to_none(features_obj, "edit_prediction_provider");
    }

    if let Some(agent) = obj.get_mut("agent")
        && let Some(agent_obj) = agent.as_object_mut()
    {
        if let Some(tools) = agent_obj
            .get_mut("tool_permissions")
            .and_then(|value| value.get_mut("tools"))
            .and_then(|value| value.as_object_mut())
        {
            tools.remove("search_web");
            tools.remove("web_search");
        }
        if let Some(profiles) = agent_obj
            .get_mut("profiles")
            .and_then(|value| value.as_object_mut())
        {
            for profile in profiles.values_mut() {
                if let Some(tools) = profile
                    .get_mut("tools")
                    .and_then(|value| value.as_object_mut())
                {
                    tools.remove("search_web");
                    tools.remove("web_search");
                }
            }
        }
    }
}

fn map_provider_to_none(obj: &mut serde_json::Map<String, Value>, field_name: &str) {
    let Some(Value::String(provider)) = obj.get(field_name) else {
        return;
    };
    if matches!(provider.as_str(), "zed" | "mercury" | "sweep") {
        obj.insert(field_name.to_owned(), Value::String("none".to_owned()));
    }
}
