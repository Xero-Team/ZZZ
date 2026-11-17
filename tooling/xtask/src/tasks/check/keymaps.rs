//! Keymap gates.
//!
//! Keymaps are JSONC, and a JSON parser silently keeps the last of two
//! identical keys in one binding object. That makes a duplicated binding
//! invisible in review and at runtime, so the only way to catch it is to parse
//! the file while rejecting repeated keys. The same parse catches binding keys
//! whose modifiers GPUI cannot recognize.
//!
//! The last gate checks the *action* side of a binding. An action name that no
//! crate declares produces a binding that never fires, and nothing else in the
//! gate suite noticed: the built-in keymaps are only validated when the app
//! starts or when `zzz`'s `test_base_keymap` runs, and CI runs neither.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt;

use anyhow::{Context as _, Result, bail};
use proc_macro2::{Delimiter, TokenTree};
use quote::ToTokens as _;
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use syn::visit::{self, Visit};
use syn::{File, ItemMacro, ItemStruct};

pub fn check() -> Result<()> {
    check_at(&super::Repo::discover()?)
}

/// The gate, rooted at an injected repository.
pub fn check_at(repo: &super::Repo) -> Result<()> {
    reject_macos_only_modifiers(repo)?;
    reject_unknown_modifiers(repo)?;
    reject_duplicate_bindings(repo)?;
    reject_unknown_actions(repo)
}

/// `cmd-` is the macOS spelling of the primary modifier. Shared and Windows
/// keymaps spell it `ctrl-`, so a `cmd-` there is a binding that never fires.
fn reject_macos_only_modifiers(repo: &super::Repo) -> Result<()> {
    let matches = repo.grep(&[
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

/// The modifiers `Keystroke::parse` recognizes
/// (`crates/gpui/src/platform/keystroke.rs`). Anything else to the left of a
/// key is folded into the key name, so a typo like `ctrol-a` produces a binding
/// that never fires. `secondary-` is platform-dependent but valid.
const RECOGNIZED_MODIFIERS: &[&str] = &[
    "ctrl",
    "alt",
    "shift",
    "fn",
    "secondary",
    "cmd",
    "super",
    "win",
];

/// Rejects binding keys whose modifiers GPUI cannot parse.
///
/// The old `reject_placeholder_modifiers` passed `super-|win-|fn-` as a single
/// `--fixed-strings` pattern: `-F` treats `|` literally, so it matched nothing
/// and the gate never fired. Turning it into an alternation would be wrong too,
/// because `super-` and `fn-` are legitimate and already shipped. Checking the
/// parsed binding keys against the parser's real modifier list is both live and
/// free of false positives.
fn reject_unknown_modifiers(repo: &super::Repo) -> Result<()> {
    let mut failures = Vec::new();
    for path in repo.tracked("assets/keymaps/*.json")? {
        let source = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let jsonc = strip_jsonc(&source);
        let document: serde_json::Value = serde_json::from_str(&jsonc)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        for key in binding_keys(&document) {
            for modifier in unknown_modifiers(&key) {
                failures.push(format!(
                    "{}: `{key}` has an unrecognized modifier `{modifier}`",
                    path.display(),
                ));
            }
        }
    }

    if failures.is_empty() {
        return Ok(());
    }
    failures.sort();
    failures.dedup();
    bail!(
        "keymap binding keys use modifiers GPUI cannot parse (the binding will never fire):\n{}",
        failures.join("\n")
    )
}

/// Returns the modifier components of a binding key that `Keystroke::parse`
/// would not recognize. The last `-` component is the key itself; `>` introduces
/// a key-char and `-` alone is a literal minus, so both are left alone.
fn unknown_modifiers(key: &str) -> Vec<String> {
    let mut findings = Vec::new();
    for stroke in key.split_whitespace() {
        let components: Vec<&str> = stroke.split('-').collect();
        for component in &components[..components.len().saturating_sub(1)] {
            if component.is_empty() || component.starts_with('>') {
                continue;
            }
            if !RECOGNIZED_MODIFIERS.contains(&component.to_ascii_lowercase().as_str()) {
                findings.push((*component).to_owned());
            }
        }
    }
    findings
}

fn binding_keys(value: &serde_json::Value) -> Vec<String> {
    let mut keys = Vec::new();
    collect_binding_keys(value, &mut keys);
    keys
}

fn collect_binding_keys(value: &serde_json::Value, keys: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, nested) in map {
                if key == "bindings"
                    && let serde_json::Value::Object(bindings) = nested
                {
                    keys.extend(bindings.keys().cloned());
                }
                collect_binding_keys(nested, keys);
            }
        }
        serde_json::Value::Array(values) => {
            for nested in values {
                collect_binding_keys(nested, keys);
            }
        }
        _ => {}
    }
}

fn reject_duplicate_bindings(repo: &super::Repo) -> Result<()> {
    let mut failures = Vec::new();

    for path in repo.tracked("assets/keymaps/*.json")? {
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

/// Rejects bindings whose action name no crate declares.
///
/// A renamed `actions!` entry (`#[action(name = "Toggle")]`) or a typo in a
/// keymap produces a binding that parses, loads and then never fires, which is
/// indistinguishable from a shortcut the user pressed wrong.
fn reject_unknown_actions(repo: &super::Repo) -> Result<()> {
    let declared = declared_actions(repo)?;
    let mut failures = Vec::new();

    for path in repo.tracked("assets/keymaps/*.json")? {
        // The storybook harness lives outside this workspace and registers its
        // own actions (`storybook::Quit`), so its keymap cannot be resolved here.
        if path
            .file_name()
            .is_some_and(|name| name == "storybook.json")
        {
            continue;
        }
        let source = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let document: serde_json::Value = serde_json::from_str(&strip_jsonc(&source))
            .with_context(|| format!("failed to parse {}", path.display()))?;
        for (keystroke, action) in binding_actions(&document) {
            if !declared.contains(&action) {
                failures.push(format!(
                    "{}: `{keystroke}` binds `{action}`, which no crate declares",
                    path.display(),
                ));
            }
        }
    }

    if failures.is_empty() {
        return Ok(());
    }
    failures.sort();
    failures.dedup();
    bail!(
        "keymap bindings name actions that no crate declares (the binding will never fire):\n{}",
        failures.join("\n")
    )
}

/// Every action name the workspace can resolve: the `actions!` entries with
/// their renames and deprecated aliases, plus `#[derive(Action)]` structs and
/// enums with their `#[action(namespace = ...)]`.
fn declared_actions(repo: &super::Repo) -> Result<BTreeSet<String>> {
    let mut sources = Vec::new();
    for path in repo.tracked("*.rs")? {
        // Vendored sources are upstream snapshots, not code this gate governs.
        if path.starts_with("vendor/") || path.starts_with("crates/grammars/vendor/") {
            continue;
        }
        let source = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if !might_declare_actions(&source) {
            continue;
        }
        sources.push((path, source));
    }

    // `macro_rules!` families expand to actions the parser cannot see, so match
    // their invocations against their definitions first; a definition may live in
    // a different file than the invocation.
    let mut families = BTreeMap::new();
    for (path, source) in &sources {
        let file: File = syn::parse_file(source)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        collect_action_macro_families(&file.items, &mut families);
    }

    let mut actions = BTreeSet::new();
    for (path, source) in &sources {
        let file: File = syn::parse_file(source)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        collect_actions_from_items(&file.items, &families, &mut actions);
    }
    Ok(actions)
}

/// A cheap pre-filter: `syn` parsing every source file in the workspace would
/// dominate the gate's runtime.
fn might_declare_actions(source: &str) -> bool {
    source.contains("actions!(") || (source.contains("derive(") && source.contains("Action"))
}

/// The actions one source file declares. Test-only: the gate itself needs the
/// cross-file [`BTreeMap`] of macro families.
#[cfg(test)]
fn declared_actions_in_source(source: &str) -> BTreeSet<String> {
    let Ok(file) = syn::parse_file(source) else {
        return BTreeSet::new();
    };
    let mut families = BTreeMap::new();
    collect_action_macro_families(&file.items, &mut families);

    let mut actions = BTreeSet::new();
    collect_actions_from_items(&file.items, &families, &mut actions);
    actions
}

fn collect_actions_from_items(
    items: &[syn::Item],
    families: &BTreeMap<String, String>,
    actions: &mut BTreeSet<String>,
) {
    let mut visitor = ActionVisitor { actions, families };
    for item in items {
        visitor.visit_item(item);
    }
}

struct ActionVisitor<'a> {
    actions: &'a mut BTreeSet<String>,
    families: &'a BTreeMap<String, String>,
}

impl<'ast> Visit<'ast> for ActionVisitor<'_> {
    fn visit_item_macro(&mut self, item: &'ast ItemMacro) {
        if let Some(segment) = item.mac.path.segments.last() {
            let name = segment.ident.to_string();
            if name == "actions" {
                collect_actions_macro(&item.mac, self.actions);
            } else if let Some(namespace) = self.families.get(&name) {
                collect_macro_family_invocation(&item.mac, namespace, self.actions);
            }
        }
        visit::visit_item_macro(self, item);
    }

    fn visit_item_struct(&mut self, item: &'ast ItemStruct) {
        collect_derived_action(&item.attrs, &item.ident, self.actions);
        visit::visit_item_struct(self, item);
    }

    fn visit_item_enum(&mut self, item: &'ast syn::ItemEnum) {
        collect_derived_action(&item.attrs, &item.ident, self.actions);
        visit::visit_item_enum(self, item);
    }
}

/// `actions!(namespace, [Plain, #[action(name = "Renamed")] Renamed, ...])`
fn collect_actions_macro(mac: &syn::Macro, actions: &mut BTreeSet<String>) {
    let mut tokens = mac.tokens.clone().into_iter();
    let Some(TokenTree::Ident(namespace)) = tokens.next() else {
        return;
    };
    let Some(TokenTree::Group(entries)) = tokens.find(|token| matches!(token, TokenTree::Group(_)))
    else {
        return;
    };
    if entries.delimiter() != Delimiter::Bracket {
        return;
    }

    let namespace = namespace.to_string();
    let mut renamed_from_attribute = None;
    let mut tokens = entries.stream().into_iter();
    while let Some(token) = tokens.next() {
        match token {
            // Attributes precede the entry they describe.
            TokenTree::Punct(punct) if punct.as_char() == '#' => {
                if let Some(TokenTree::Group(attribute)) = tokens.next() {
                    let effects = parse_action_attribute(attribute.stream());
                    for alias in effects.deprecated_aliases {
                        actions.insert(alias);
                    }
                    renamed_from_attribute = effects.name;
                }
            }
            TokenTree::Ident(identifier) => {
                let name = renamed_from_attribute
                    .take()
                    .unwrap_or_else(|| identifier.to_string());
                actions.insert(format!("{namespace}::{name}"));
            }
            _ => {}
        }
    }
}

/// A `macro_rules!` definition whose expansion contains `#[derive(Action)]`
/// with a namespace, such as `split_structs!` in the workspace crate. `syn` only
/// sees the definition, never the structs it expands to, so the invocation is
/// what tells the gate which actions exist.
fn collect_action_macro_families(items: &[syn::Item], families: &mut BTreeMap<String, String>) {
    for item in items {
        match item {
            syn::Item::Macro(item_macro) if item_macro.mac.path.is_ident("macro_rules") => {
                if let Some((name, namespace)) = action_macro_family(item_macro) {
                    families.insert(name, namespace);
                }
            }
            syn::Item::Mod(item_mod) => {
                if let Some((_, items)) = &item_mod.content {
                    collect_action_macro_families(items, families);
                }
            }
            _ => {}
        }
    }
}

fn action_macro_family(item: &ItemMacro) -> Option<(String, String)> {
    // `syn` keeps the macro's name in `ident`; `mac.tokens` starts at the
    // matcher, so the definition's body is the whole token stream.
    let name = item.ident.as_ref()?.to_string();
    let tokens = item.mac.tokens.clone();
    if !contains_ident(tokens.clone(), "Action") {
        return None;
    }
    Some((name, find_namespace(tokens)?))
}

fn contains_ident(tokens: proc_macro2::TokenStream, wanted: &str) -> bool {
    tokens.into_iter().any(|token| match token {
        TokenTree::Ident(identifier) => identifier == wanted,
        TokenTree::Group(group) => contains_ident(group.stream(), wanted),
        _ => false,
    })
}

/// The `N` of an `#[action(namespace = N)]` anywhere in the token tree.
fn find_namespace(tokens: proc_macro2::TokenStream) -> Option<String> {
    let tokens: Vec<TokenTree> = tokens.into_iter().collect();
    let mut index = 0;
    while index < tokens.len() {
        match &tokens[index] {
            TokenTree::Group(group) => {
                if let Some(namespace) = find_namespace(group.stream()) {
                    return Some(namespace);
                }
            }
            TokenTree::Ident(identifier) if identifier == "namespace" => {
                if let (Some(TokenTree::Punct(punct)), Some(TokenTree::Ident(namespace))) =
                    (tokens.get(index + 1), tokens.get(index + 2))
                    && punct.as_char() == '='
                {
                    return Some(namespace.to_string());
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// `split_structs!(SplitLeft => "...", SplitRight => "...")`: the first
/// identifier of each comma-separated entry names one generated action.
fn collect_macro_family_invocation(
    mac: &syn::Macro,
    namespace: &str,
    actions: &mut BTreeSet<String>,
) {
    let tokens: Vec<TokenTree> = mac.tokens.clone().into_iter().collect();
    let arguments: Vec<TokenTree> = match tokens.as_slice() {
        [TokenTree::Group(group)] => group.stream().into_iter().collect(),
        _ => tokens,
    };

    let mut at_entry_start = true;
    for token in arguments {
        match token {
            TokenTree::Ident(identifier) if at_entry_start => {
                actions.insert(format!("{namespace}::{identifier}"));
                at_entry_start = false;
            }
            TokenTree::Punct(punct) if punct.as_char() == ',' => at_entry_start = true,
            _ => at_entry_start = false,
        }
    }
}

/// `#[derive(..., Action)] #[action(namespace = namespace)] struct Name`
fn collect_derived_action(
    attributes: &[syn::Attribute],
    identifier: &syn::Ident,
    actions: &mut BTreeSet<String>,
) {
    if !attributes.iter().any(derives_action) {
        return;
    }

    let mut name = None;
    let mut namespace = None;
    for attribute in attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("action"))
    {
        let effects = parse_action_attribute(attribute.meta.to_token_stream());
        if effects.name.is_some() {
            name = effects.name;
        }
        if effects.namespace.is_some() {
            namespace = effects.namespace;
        }
        for alias in effects.deprecated_aliases {
            actions.insert(alias);
        }
    }

    let name = name.unwrap_or_else(|| identifier.to_string());
    match namespace {
        Some(namespace) => actions.insert(format!("{namespace}::{name}")),
        None => actions.insert(name),
    };
}

/// `#[derive(..., Action)]` — the derive may be written `gpui::Action`, and it
/// may appear anywhere in the list.
fn derives_action(attribute: &syn::Attribute) -> bool {
    if !attribute.path().is_ident("derive") {
        return false;
    }
    let Ok(paths) = attribute.parse_args_with(
        syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
    ) else {
        return false;
    };
    paths.iter().any(|path| {
        path.segments
            .last()
            .is_some_and(|segment| segment.ident == "Action")
    })
}

#[derive(Default)]
struct ActionAttribute {
    /// `#[action(name = "...")]`, which overrides the Rust identifier.
    name: Option<String>,
    /// `#[action(namespace = ...)]`, the prefix of a derived action.
    namespace: Option<String>,
    /// `#[action(deprecated_aliases = ["old::Name", ...])]`, which stay valid.
    deprecated_aliases: Vec<String>,
}

impl ActionAttribute {
    fn absorb(&mut self, other: ActionAttribute) {
        if other.name.is_some() {
            self.name = other.name;
        }
        if other.namespace.is_some() {
            self.namespace = other.namespace;
        }
        self.deprecated_aliases.extend(other.deprecated_aliases);
    }
}

/// Reads `name = "..."`, `namespace = ...` and `deprecated_aliases = [...]`
/// from the tokens of `#[action(...)]`. The arguments arrive nested in a
/// parenthesis group, so nested groups are visited too.
fn parse_action_attribute(tokens: impl IntoIterator<Item = TokenTree>) -> ActionAttribute {
    let mut attribute = ActionAttribute::default();
    let tokens: Vec<TokenTree> = tokens.into_iter().collect();
    let mut index = 0;

    while index < tokens.len() {
        match &tokens[index] {
            TokenTree::Group(group) => {
                attribute.absorb(parse_action_attribute(group.stream()));
                index += 1;
            }
            TokenTree::Ident(identifier) => {
                let is_assignment = matches!(
                    tokens.get(index + 1),
                    Some(TokenTree::Punct(punct)) if punct.as_char() == '='
                );
                if !is_assignment {
                    index += 1;
                    continue;
                }
                match (identifier.to_string().as_str(), tokens.get(index + 2)) {
                    ("name", Some(TokenTree::Literal(literal))) => {
                        attribute.name = literal_string(literal);
                    }
                    ("namespace", Some(TokenTree::Ident(namespace))) => {
                        attribute.namespace = Some(namespace.to_string());
                    }
                    ("deprecated_aliases", Some(TokenTree::Group(aliases))) => {
                        for alias in aliases.stream() {
                            if let TokenTree::Literal(literal) = alias
                                && let Some(alias) = literal_string(&literal)
                            {
                                attribute.deprecated_aliases.push(alias);
                            }
                        }
                    }
                    _ => {}
                }
                index += 3;
            }
            _ => index += 1,
        }
    }

    attribute
}

/// The literal's value, so `"\u{54}oggle"` is read the way rustc reads it.
fn literal_string(literal: &proc_macro2::Literal) -> Option<String> {
    syn::parse_str::<syn::LitStr>(&literal.to_string())
        .ok()
        .map(|literal| literal.value())
}

/// The `(keystroke, action)` pairs of every `bindings` object in the document.
fn binding_actions(value: &serde_json::Value) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    collect_binding_actions(value, &mut pairs);
    pairs
}

fn collect_binding_actions(value: &serde_json::Value, pairs: &mut Vec<(String, String)>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, nested) in map {
                if key == "bindings"
                    && let serde_json::Value::Object(bindings) = nested
                {
                    for (keystroke, action) in bindings {
                        if let Some(action) = action_name(action) {
                            pairs.push((keystroke.clone(), action));
                        }
                    }
                }
                collect_binding_actions(nested, pairs);
            }
        }
        serde_json::Value::Array(values) => {
            for nested in values {
                collect_binding_actions(nested, pairs);
            }
        }
        _ => {}
    }
}

/// A binding is either `"namespace::Action"` or `["namespace::Action", {args}]`.
fn action_name(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(name) => Some(name.clone()),
        serde_json::Value::Array(items) => items.first()?.as_str().map(str::to_owned),
        _ => None,
    }
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
pub(super) struct NoDuplicateKeys;

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
                return Err(de::Error::custom(format!("duplicate key {key:?}")));
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
    use super::{
        NoDuplicateKeys, action_name, declared_actions_in_source, strip_jsonc, unknown_modifiers,
    };

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
            error.to_string().contains("duplicate key"),
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

    #[test]
    fn accepts_recognized_modifiers() {
        for key in [
            "ctrl-a",
            "cmd-shift-p",
            "ctrl-alt-super-p",
            "fn-f",
            "secondary-k",
            "cmd-k cmd-s",
        ] {
            assert!(
                unknown_modifiers(key).is_empty(),
                "{key} should be accepted: {:?}",
                unknown_modifiers(key)
            );
        }
    }

    #[test]
    fn rejects_an_unknown_modifier() {
        assert_eq!(unknown_modifiers("ctrol-a"), vec!["ctrol"]);
        assert_eq!(unknown_modifiers("ctrl-hyper-a"), vec!["hyper"]);
    }

    #[test]
    fn ignores_keys_without_modifiers_and_key_chars() {
        // The last component is the key, `>` introduces a key-char, and a bare
        // `-` is a literal minus; none of these are modifier typos.
        assert!(unknown_modifiers("a").is_empty());
        assert!(unknown_modifiers("g >").is_empty());
        assert!(unknown_modifiers("-").is_empty());
        assert!(unknown_modifiers("[ -").is_empty());
    }

    #[test]
    fn collects_actions_macro_entries() {
        let actions = declared_actions_in_source(
            r#"
            actions!(
                editor,
                [
                    /// A doc comment, which is an attribute too.
                    SelectAll,
                    #[action(name = "Toggle")]
                    ToggleComments,
                    #[action(deprecated_aliases = ["project_panel::CopyPath"])]
                    CopyPath,
                ]
            );
            "#,
        );
        assert!(actions.contains("editor::SelectAll"), "{actions:?}");
        assert!(actions.contains("editor::Toggle"), "{actions:?}");
        assert!(!actions.contains("editor::ToggleComments"), "{actions:?}");
        assert!(actions.contains("project_panel::CopyPath"), "{actions:?}");
    }

    #[test]
    fn collects_macro_generated_actions() {
        let source = r#"
            macro_rules! split_structs {
                ($($name:ident => $doc:literal),* $(,)?) => {
                    $(
                        #[doc = $doc]
                        #[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
                        #[action(namespace = pane)]
                        pub struct $name {
                            pub mode: usize,
                        }
                    )*
                };
            }

            split_structs!(
                SplitLeft => "Splits the pane to the left.",
                SplitDown => "Splits the pane downward.",
            );
            "#;
        let file = syn::parse_file(source).expect("fixture should parse");
        let definition = match &file.items[0] {
            syn::Item::Macro(item) => item,
            other => panic!("unexpected item {other:?}"),
        };
        println!(
            "is_macro_rules={} family={:?}",
            definition.mac.path.is_ident("macro_rules"),
            super::action_macro_family(definition),
        );
        let actions = declared_actions_in_source(source);
        assert!(actions.contains("pane::SplitLeft"), "{actions:?}");
        assert!(actions.contains("pane::SplitDown"), "{actions:?}");
    }

    #[test]
    fn collects_derived_actions() {
        let actions = declared_actions_in_source(
            r#"
            /// A struct action flattened out of its own derive list.
            #[derive(PartialEq, Clone, Default, Debug, Deserialize, JsonSchema, Action)]
            #[action(namespace = editor)]
            #[serde(deny_unknown_fields)]
            pub struct AddSelectionAbove {
                pub count: usize,
            }

            #[derive(Clone, PartialEq, Debug, Action)]
            pub struct Unnamespaced;
            "#,
        );
        assert!(actions.contains("editor::AddSelectionAbove"), "{actions:?}");
        assert!(actions.contains("Unnamespaced"), "{actions:?}");
    }

    #[test]
    fn reads_binding_actions_from_both_shapes() {
        let document: serde_json::Value = serde_json::from_str(
            r#"{
                "bindings": {
                    "ctrl-a": "editor::SelectAll",
                    "ctrl-b": ["editor::CopyWithArgs", { "count": 2 }]
                }
            }"#,
        )
        .expect("fixture should parse");
        let mut pairs = super::binding_actions(&document);
        pairs.sort();
        assert_eq!(
            pairs,
            vec![
                ("ctrl-a".to_owned(), "editor::SelectAll".to_owned()),
                ("ctrl-b".to_owned(), "editor::CopyWithArgs".to_owned()),
            ]
        );
        assert_eq!(action_name(&serde_json::Value::Null), None);
    }
}
