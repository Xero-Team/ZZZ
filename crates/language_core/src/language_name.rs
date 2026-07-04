use gpui_shared_string::SharedString;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    borrow::Borrow,
    sync::atomic::{AtomicUsize, Ordering::SeqCst},
};

static NEXT_LANGUAGE_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct LanguageId(usize);

impl LanguageId {
    pub fn new() -> Self {
        Self(NEXT_LANGUAGE_ID.fetch_add(1, SeqCst))
    }
}

impl Default for LanguageId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(
    Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub struct LanguageName(pub SharedString);

impl LanguageName {
    pub fn new(s: &str) -> Self {
        Self(SharedString::new(s))
    }

    pub fn new_static(s: &'static str) -> Self {
        Self(SharedString::new_static(s))
    }

    pub fn from_proto(s: String) -> Self {
        Self(SharedString::from(s))
    }

    pub fn to_proto(&self) -> String {
        self.0.to_string()
    }

    pub fn lsp_id(&self) -> String {
        match self.0.as_ref() {
            "Plain Text" => "plaintext".to_owned(),
            language_name => language_name.to_lowercase(),
        }
    }
}

impl From<LanguageName> for SharedString {
    fn from(value: LanguageName) -> Self {
        value.0
    }
}

impl From<SharedString> for LanguageName {
    fn from(value: SharedString) -> Self {
        LanguageName(value)
    }
}

impl AsRef<str> for LanguageName {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl Borrow<str> for LanguageName {
    fn borrow(&self) -> &str {
        self.0.as_ref()
    }
}

impl PartialEq<str> for LanguageName {
    fn eq(&self, other: &str) -> bool {
        self.0.as_ref() == other
    }
}

impl PartialEq<&str> for LanguageName {
    fn eq(&self, other: &&str) -> bool {
        self.0.as_ref() == *other
    }
}

impl std::fmt::Display for LanguageName {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&'static str> for LanguageName {
    fn from(str: &'static str) -> Self {
        Self(SharedString::new_static(str))
    }
}

impl From<LanguageName> for String {
    fn from(value: LanguageName) -> Self {
        let value: &str = &value.0;
        Self::from(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_id_increments_monotonically() {
        let first = LanguageId::new();
        let second = LanguageId::new();
        assert!(first < second);
    }

    #[test]
    fn language_name_helpers_convert_and_normalize_lsp_ids() {
        let rust = LanguageName::new("Rust");
        assert_eq!(rust.as_ref(), "Rust");
        assert_eq!(rust.to_proto(), "Rust");
        assert_eq!(rust.lsp_id(), "rust");
        assert_eq!(rust.to_string(), "Rust");
        assert_eq!(String::from(LanguageName::new("Rust")), "Rust");
        assert_eq!(
            SharedString::from(LanguageName::new("Rust")),
            SharedString::new("Rust")
        );

        let plain_text = LanguageName::new_static("Plain Text");
        assert_eq!(plain_text.lsp_id(), "plaintext");

        let from_proto = LanguageName::from_proto("TypeScript".into());
        assert_eq!(from_proto, "TypeScript");
    }
}
