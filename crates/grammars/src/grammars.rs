use anyhow::Context as _;
use language_core::{LanguageConfig, LanguageQueries, QUERY_FILENAME_PREFIXES};
use rust_embed::RustEmbed;
use std::borrow::Cow;
use util::asset_str;

#[derive(RustEmbed)]
#[folder = "src/"]
#[exclude = "*.rs"]
struct GrammarDir;

/// Register all built-in native tree-sitter grammars with the provided registration function.
///
/// Each grammar is registered as a `(&str, tree_sitter_language::LanguageFn)` pair.
/// This must be called before loading language configs/queries.
#[cfg(feature = "load-grammars")]
pub fn native_grammars() -> Vec<(&'static str, tree_sitter::Language)> {
    vec![
        ("bash", tree_sitter_bash::LANGUAGE.into()),
        ("c", tree_sitter_c::LANGUAGE.into()),
        ("cpp", tree_sitter_cpp::LANGUAGE.into()),
        ("css", tree_sitter_css::LANGUAGE.into()),
        ("dtd", tree_sitter_xml::LANGUAGE_DTD.into()),
        ("diff", tree_sitter_diff::LANGUAGE.into()),
        ("gitattributes", tree_sitter_gitattributes::LANGUAGE.into()),
        ("go", tree_sitter_go::LANGUAGE.into()),
        ("gomod", tree_sitter_go_mod::LANGUAGE.into()),
        ("gowork", tree_sitter_gowork::LANGUAGE.into()),
        ("jsdoc", tree_sitter_jsdoc::LANGUAGE.into()),
        ("json", tree_sitter_json::LANGUAGE.into()),
        ("jsonc", tree_sitter_json::LANGUAGE.into()),
        ("markdown", tree_sitter_md::LANGUAGE.into()),
        ("markdown-inline", tree_sitter_md::INLINE_LANGUAGE.into()),
        ("python", tree_sitter_python::LANGUAGE.into()),
        ("regex", tree_sitter_regex::LANGUAGE.into()),
        ("rust", tree_sitter_rust::LANGUAGE.into()),
        ("tsx", tree_sitter_typescript::LANGUAGE_TSX.into()),
        (
            "typescript",
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        ),
        ("xml", tree_sitter_xml::LANGUAGE_XML.into()),
        ("yaml", tree_sitter_yaml::LANGUAGE.into()),
        ("gitcommit", tree_sitter_gitcommit::LANGUAGE.into()),
    ]
}

/// Load and parse the `config.toml` for a given language name.
pub fn load_config(name: &str) -> LanguageConfig {
    let config_toml = String::from_utf8(
        GrammarDir::get(&format!("{}/config.toml", name))
            .unwrap_or_else(|| panic!("missing config for language {:?}", name))
            .data
            .to_vec(),
    )
    .unwrap();

    let config: LanguageConfig = ::toml::from_str(&config_toml)
        .with_context(|| format!("failed to load config.toml for language {name:?}"))
        .unwrap();

    config
}

/// Load and parse the `config.toml` for a given language name, stripping fields
/// that require grammar support when grammars are not loaded.
pub fn load_config_for_feature(name: &str, grammars_loaded: bool) -> LanguageConfig {
    let config = load_config(name);

    if grammars_loaded {
        config
    } else {
        LanguageConfig {
            name: config.name,
            matcher: config.matcher,
            query_layers: config.query_layers,
            jsx_tag_auto_close: config.jsx_tag_auto_close,
            ..Default::default()
        }
    }
}

/// Get a raw embedded file by path (relative to `src/`).
///
/// Returns the file data as bytes, or `None` if the file does not exist.
pub fn get_file(path: &str) -> Option<rust_embed::EmbeddedFile> {
    GrammarDir::get(path)
}

/// Load all `.scm` query files for a given language name into a `LanguageQueries`.
///
/// Multiple `.scm` files with the same prefix (e.g. `highlights.scm` and
/// `highlights_extra.scm`) are concatenated together with their contents appended.
pub fn load_queries(name: &str) -> LanguageQueries {
    load_queries_from_prefixes(&[name])
}

pub fn load_queries_for_config(name: &str, config: &LanguageConfig) -> LanguageQueries {
    let mut prefixes = config
        .query_layers
        .iter()
        .map(|layer| layer.as_ref())
        .collect::<Vec<_>>();
    prefixes.push(name);
    load_queries_from_prefixes(&prefixes)
}

pub fn load_queries_from_prefixes(prefixes: &[&str]) -> LanguageQueries {
    let mut result = LanguageQueries::default();
    for prefix in prefixes {
        append_queries_from_prefix(&mut result, prefix);
    }
    result
}

fn append_queries_from_prefix(result: &mut LanguageQueries, prefix: &str) {
    for path in GrammarDir::iter() {
        if let Some(remainder) = path.strip_prefix(prefix).and_then(|p| p.strip_prefix('/')) {
            if !remainder.ends_with(".scm") {
                continue;
            }
            for (query_prefix, query) in QUERY_FILENAME_PREFIXES {
                if remainder.starts_with(query_prefix) {
                    append_query(query(result), asset_str::<GrammarDir>(path.as_ref()));
                }
            }
        }
    }
}

fn append_query(target: &mut Option<Cow<'static, str>>, contents: Cow<'static, str>) {
    match target {
        None => *target = Some(contents),
        Some(existing) => {
            if !existing.is_empty() {
                existing.to_mut().push('\n');
            }
            existing.to_mut().push_str(contents.as_ref());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_queries_include_shared_and_xml_specific_layers() {
        let config = load_config("xml");
        let queries = load_queries_for_config("xml", &config);

        let highlights = queries.highlights.expect("xml highlights query");
        assert!(highlights.contains("(elementdecl"));
        assert!(highlights.contains("(STag (Name) @tag)"));

        let outline = queries.outline.expect("xml outline query");
        assert!(outline.contains("(element"));
    }

    #[test]
    fn xsd_queries_include_xml_and_xsd_layers() {
        let config = load_config("xsd");
        let queries = load_queries_for_config("xsd", &config);

        let highlights = queries.highlights.expect("xsd highlights query");
        assert!(highlights.contains("(STag (Name) @tag)"));
        assert!(highlights.contains("complexType"));

        let indents = queries.indents.expect("xsd indents query");
        assert!(indents.contains("(element"));
    }
}
