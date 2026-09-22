use std::{
    borrow::{Borrow, Cow},
    iter,
    sync::Arc,
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// A shared string is an immutable string that can be cheaply cloned in GPUI
/// tasks. Essentially an abstraction over an `Arc<str>` and `&'static str`,
/// currently backed by a [`SmolStr`].
#[derive(Eq, PartialEq, PartialOrd, Ord, Hash, Clone)]
pub struct SharedString(SmolStr);

impl std::ops::Deref for SharedString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_str()
    }
}

impl SharedString {
    /// Creates a static [`SharedString`] from a `&'static str`.
    pub const fn new_static(str: &'static str) -> Self {
        Self(SmolStr::new_static(str))
    }

    /// Creates a [`SharedString`].
    pub fn new(str: impl AsRef<str>) -> Self {
        SharedString(SmolStr::new(str))
    }

    /// Get a &str from the underlying string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl JsonSchema for SharedString {
    fn inline_schema() -> bool {
        String::inline_schema()
    }

    fn schema_name() -> Cow<'static, str> {
        String::schema_name()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        String::json_schema(generator)
    }
}

impl Default for SharedString {
    fn default() -> Self {
        Self::new_static("")
    }
}

impl AsRef<str> for SharedString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for SharedString {
    fn borrow(&self) -> &str {
        self.as_ref()
    }
}

impl std::fmt::Debug for SharedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Display for SharedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.as_str())
    }
}

impl PartialEq<String> for SharedString {
    fn eq(&self, other: &String) -> bool {
        self.as_ref() == other
    }
}

impl PartialEq<SharedString> for String {
    fn eq(&self, other: &SharedString) -> bool {
        self == other.as_ref()
    }
}

impl PartialEq<str> for SharedString {
    fn eq(&self, other: &str) -> bool {
        self.as_ref() == other
    }
}

impl<'a> PartialEq<&'a str> for SharedString {
    fn eq(&self, other: &&'a str) -> bool {
        self.as_ref() == *other
    }
}

impl From<&SharedString> for SharedString {
    #[inline]
    fn from(s: &SharedString) -> SharedString {
        s.clone()
    }
}

impl From<&str> for SharedString {
    #[inline]
    fn from(s: &str) -> SharedString {
        SharedString(SmolStr::from(s))
    }
}

impl From<char> for SharedString {
    #[inline]
    fn from(c: char) -> SharedString {
        SharedString(SmolStr::from_iter(iter::once(c)))
    }
}

impl From<&mut str> for SharedString {
    #[inline]
    fn from(s: &mut str) -> SharedString {
        SharedString(SmolStr::from(s))
    }
}

impl From<&String> for SharedString {
    #[inline]
    fn from(s: &String) -> SharedString {
        SharedString(SmolStr::from(s))
    }
}

impl From<String> for SharedString {
    #[inline(always)]
    fn from(text: String) -> Self {
        SharedString(SmolStr::from(text))
    }
}

impl From<Box<str>> for SharedString {
    #[inline]
    fn from(s: Box<str>) -> SharedString {
        SharedString(SmolStr::from(s))
    }
}

impl From<Arc<str>> for SharedString {
    #[inline]
    fn from(s: Arc<str>) -> SharedString {
        SharedString(SmolStr::from(s))
    }
}

impl From<&Arc<str>> for SharedString {
    #[inline]
    fn from(s: &Arc<str>) -> SharedString {
        SharedString(SmolStr::from(s.clone()))
    }
}

impl<'a> From<Cow<'a, str>> for SharedString {
    #[inline]
    fn from(s: Cow<'a, str>) -> SharedString {
        SharedString(SmolStr::from(s))
    }
}

impl From<SharedString> for Arc<str> {
    #[inline(always)]
    fn from(text: SharedString) -> Self {
        text.0.into()
    }
}

impl From<SharedString> for String {
    #[inline(always)]
    fn from(text: SharedString) -> Self {
        text.0.into()
    }
}

impl Serialize for SharedString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_ref())
    }
}

impl<'de> Deserialize<'de> for SharedString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(SharedString::new(&s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_and_static_values_are_empty() {
        assert_eq!(SharedString::default().as_str(), "");
        assert_eq!(SharedString::new_static("static").as_str(), "static");
    }

    #[test]
    fn conversions_preserve_contents() {
        let owned = String::from("owned");
        let shared_from_owned = SharedString::from(owned.clone());
        let shared_from_ref = SharedString::from(&owned);
        let shared_from_cow = SharedString::from(Cow::Borrowed("borrowed"));
        let shared_from_arc = SharedString::from(Arc::<str>::from("arc"));

        assert_eq!(shared_from_owned.as_str(), "owned");
        assert_eq!(shared_from_ref.as_str(), "owned");
        assert_eq!(shared_from_cow.as_str(), "borrowed");
        assert_eq!(shared_from_arc.as_str(), "arc");
        assert_eq!(String::from(shared_from_owned), "owned");
    }

    #[test]
    fn comparisons_match_string_types() {
        let shared = SharedString::new("value");

        assert_eq!(shared, "value");
        assert_eq!(shared, String::from("value"));
        assert_eq!(String::from("value"), shared);
    }

    #[test]
    fn serde_round_trips_as_a_json_string() {
        let shared = SharedString::new("hello");

        let json = serde_json::to_string(&shared).expect("serialize shared string");
        let deserialized: SharedString =
            serde_json::from_str(&json).expect("deserialize shared string");

        assert_eq!(json, "\"hello\"");
        assert_eq!(deserialized, shared);
    }
}
