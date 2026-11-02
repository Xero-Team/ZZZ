//! Keymap gates.
//!
//! Keymaps are JSONC, and a JSON parser silently keeps the last of two
//! identical keys in one binding object. That makes a duplicated binding
//! invisible in review and at runtime, so the only way to catch it is to parse
//! the file while rejecting repeated keys.

use std::collections::HashSet;
use std::fmt;

use anyhow::{Context as _, Result, bail};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};

pub fn check() -> Result<()> {
    reject_macos_only_modifiers()?;
    reject_placeholder_modifiers()?;
    reject_duplicate_bindings()
}

/// `cmd-` is the macOS spelling of the primary modifier. Shared and Windows
/// keymaps spell it `ctrl-`, so a `cmd-` there is a binding that never fires.
fn reject_macos_only_modifiers() -> Result<()> {
    let matches = super::git_grep(&[
        "--no-color",
        "--line-number",
        "--fixed-strings",
        "-e",
        "cmd-",
        "--",
        "assets/keymaps/",
        ":(exclude)assets/keymaps/storybook.json",
        ":(exclude)assets/keymaps/default-macos.json",
        ":(exclude)assets/keymaps/specific-overrides-macos.json",
        ":(exclude)assets/keymaps/macos/*.json",
    ])?;

    if matches.is_empty() {
        return Ok(());
    }
    bail!("found `cmd-` in non-macOS keymap files:\n{matches}");
}

/// Kept as a fixed-string search on purpose: the original check used `-F`, and
/// `super-` and `fn-` are legitimately in use (`ctrl-alt-super-p`, `fn-f`), so
/// treating this as an alternation would reject the shipped keymaps.
fn reject_placeholder_modifiers() -> Result<()> {
    let matches = super::git_grep(&[
        "--no-color",
        "--line-number",
        "--fixed-strings",
        "-e",
        "super-|win-|fn-",
        "--",
        "assets/keymaps/",
    ])?;

    if matches.is_empty() {
        return Ok(());
    }
    bail!("found placeholder modifiers in keymap files:\n{matches}");
}

fn reject_duplicate_bindings() -> Result<()> {
    let mut failures = Vec::new();

    for path in super::tracked_files("assets/keymaps/*.json")? {
        let source = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let jsonc = strip_jsonc(&source);
        if let Err(error) = serde_json::from_str::<NoDuplicateKeys>(&jsonc) {
            failures.push(format!("{}: {error}", path.display()));
        }
    }

    if failures.is_empty() {
        return Ok(());
    }
    bail!(
        "duplicate keybindings found in the same context:\n{}",
        failures.join("\n")
    );
}

/// Removes `//` and `/* */` comments and the trailing commas that JSONC allows,
/// because `serde_json` accepts neither. Comment markers inside strings are
/// preserved.
fn strip_jsonc(source: &str) -> String {
    use regex::Regex;

    let mut stripped = String::with_capacity(source.len());
    let mut characters = source.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(character) = characters.next() {
        if in_string {
            stripped.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }

        match character {
            '"' => {
                in_string = true;
                stripped.push(character);
            }
            '/' if characters.peek() == Some(&'/') => {
                for next in characters.by_ref() {
                    if next == '\n' {
                        stripped.push('\n');
                        break;
                    }
                }
            }
            '/' if characters.peek() == Some(&'*') => {
                characters.next();
                let mut previous = '\0';
                for next in characters.by_ref() {
                    if previous == '*' && next == '/' {
                        break;
                    }
                    previous = next;
                }
            }
            _ => stripped.push(character),
        }
    }

    static TRAILING_COMMA: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r",(\s*[}\]])").expect("valid pattern"));
    TRAILING_COMMA.replace_all(&stripped, "$1").into_owned()
}

/// Deserializes any JSON shape, failing when one object repeats a key.
struct NoDuplicateKeys;

impl<'de> Deserialize<'de> for NoDuplicateKeys {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(NoDuplicateKeysVisitor)
    }
}

struct NoDuplicateKeysVisitor;

impl<'de> Visitor<'de> for NoDuplicateKeysVisitor {
    type Value = NoDuplicateKeys;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value whose objects do not repeat a key")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut seen = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !seen.insert(key.clone()) {
                return Err(de::Error::custom(format!("duplicate binding {key:?}")));
            }
            map.next_value::<NoDuplicateKeys>()?;
        }
        Ok(NoDuplicateKeys)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence.next_element::<NoDuplicateKeys>()?.is_some() {}
        Ok(NoDuplicateKeys)
    }

    fn visit_str<E: de::Error>(self, _value: &str) -> Result<Self::Value, E> {
        Ok(NoDuplicateKeys)
    }

    fn visit_string<E: de::Error>(self, _value: String) -> Result<Self::Value, E> {
        Ok(NoDuplicateKeys)
    }

    fn visit_bool<E: de::Error>(self, _value: bool) -> Result<Self::Value, E> {
        Ok(NoDuplicateKeys)
    }

    fn visit_i64<E: de::Error>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(NoDuplicateKeys)
    }

    fn visit_u64<E: de::Error>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(NoDuplicateKeys)
    }

    fn visit_f64<E: de::Error>(self, _value: f64) -> Result<Self::Value, E> {
        Ok(NoDuplicateKeys)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(NoDuplicateKeys)
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(NoDuplicateKeys)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(self)
    }
}

#[cfg(test)]
mod tests {
    use super::{NoDuplicateKeys, strip_jsonc};

    fn parse(source: &str) -> Result<(), serde_json::Error> {
        serde_json::from_str::<NoDuplicateKeys>(&strip_jsonc(source)).map(|_| ())
    }

    #[test]
    fn accepts_jsonc_comments_and_trailing_commas() {
        parse(
            r#"{
              // a line comment
              "context": "Editor",
              /* a block comment */
              "bindings": {
                "ctrl-a": "editor::SelectAll",
              },
            }"#,
        )
        .expect("JSONC with comments and trailing commas should parse");
    }

    #[test]
    fn rejects_a_duplicate_binding() {
        let error = parse(r#"{"bindings": {"ctrl-a": "a", "ctrl-a": "b"}}"#)
            .expect_err("a repeated key must fail");
        assert!(
            error.to_string().contains("duplicate binding"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_a_duplicate_binding_in_a_nested_object() {
        parse(r#"{"bindings": {"ctrl-a": {"ctrl-b": "x", "ctrl-b": "y"}}}"#)
            .expect_err("a repeated key in a nested object must fail");
    }

    #[test]
    fn keeps_comment_markers_inside_strings() {
        parse(r#"{"url": "https://example.com/path"}"#)
            .expect("`//` inside a string is not a comment");
    }
}
