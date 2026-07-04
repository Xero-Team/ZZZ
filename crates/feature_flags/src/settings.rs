use collections::HashMap;
use schemars::{Schema, json_schema};
use serde_json::{Map, Value};
use settings::{RegisterSetting, Settings, SettingsContent};

use crate::FeatureFlagStore;

#[derive(Clone, Debug, Default, RegisterSetting)]
pub struct FeatureFlagsSettings {
    pub overrides: HashMap<String, String>,
}

impl Settings for FeatureFlagsSettings {
    fn from_settings(content: &SettingsContent) -> Self {
        Self {
            overrides: content
                .feature_flags
                .as_ref()
                .map(|map| map.0.clone())
                .unwrap_or_default(),
        }
    }
}

/// Produces a JSON schema for the `feature_flags` object that lists each known
/// flag as a property with its variant keys as an `enum`.
///
/// Unknown flags are permitted via `additionalProperties: { "type": "string" }`,
/// so removing a flag from the binary never turns existing entries in
/// `settings.json` into validation errors.
pub fn generate_feature_flags_schema() -> Schema {
    let mut properties = Map::new();

    for descriptor in FeatureFlagStore::known_flags() {
        let variants = (descriptor.variants)();
        let enum_values: Vec<Value> = variants
            .iter()
            .map(|v| Value::String(v.override_key.to_owned()))
            .collect();
        let enum_descriptions: Vec<Value> = variants
            .iter()
            .map(|v| Value::String(v.label.to_owned()))
            .collect();

        let mut property = Map::new();
        property.insert("type".to_owned(), Value::String("string".to_owned()));
        property.insert("enum".to_owned(), Value::Array(enum_values));
        // VS Code / json-language-server use `enumDescriptions` for hover docs
        // on each enum value; schemars passes them through untouched.
        property.insert(
            "enumDescriptions".to_owned(),
            Value::Array(enum_descriptions),
        );
        property.insert(
            "description".to_owned(),
            Value::String(format!(
                "Override for the `{}` feature flag. Default: `{}` (the {} variant).",
                descriptor.name,
                (descriptor.default_variant_key)(),
                (descriptor.default_variant_key)(),
            )),
        );

        properties.insert(descriptor.name.to_owned(), Value::Object(property));
    }

    json_schema!({
        "type": "object",
        "description": "Local overrides for feature flags, keyed by flag name.",
        "properties": properties,
        "additionalProperties": {
            "type": "string",
            "description": "Unknown feature flag; retained so removed flags don't trip settings validation."
        }
    })
}

#[cfg(test)]
mod tests {
    use super::generate_feature_flags_schema;

    #[test]
    fn generated_schema_includes_known_flags_and_unknown_string_fallback() {
        let schema = serde_json::to_value(generate_feature_flags_schema()).unwrap();

        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"]["type"], "string");

        let panic_flag = &schema["properties"]["panic"];
        assert_eq!(panic_flag["type"], "string");
        assert_eq!(panic_flag["enum"], serde_json::json!(["on", "off"]));
        assert_eq!(
            panic_flag["enumDescriptions"],
            serde_json::json!(["On", "Off"])
        );
        assert!(
            panic_flag["description"]
                .as_str()
                .is_some_and(|description| description.contains("Default: `off`"))
        );
    }
}
