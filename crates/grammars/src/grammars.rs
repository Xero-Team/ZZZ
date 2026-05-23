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
        ("git_config", tree_sitter_git_config::LANGUAGE.into()),
        ("gitattributes", tree_sitter_gitattributes::LANGUAGE.into()),
        ("gitignore", tree_sitter_gitignore::LANGUAGE.into()),
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
        ("syslog", tree_sitter_syslog::LANGUAGE.into()),
        ("toml", tree_sitter_toml::LANGUAGE.into()),
        ("tsx", tree_sitter_typescript::LANGUAGE_TSX.into()),
        (
            "typescript",
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        ),
        ("xml", tree_sitter_xml::LANGUAGE_XML.into()),
        ("yaml", tree_sitter_yaml::LANGUAGE.into()),
        ("gitcommit", tree_sitter_gitcommit::LANGUAGE.into()),
        ("git_rebase", tree_sitter_git_rebase::LANGUAGE.into()),
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
    use tree_sitter::Parser;

    fn parse_toml(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_toml::LANGUAGE.into())
            .expect("load TOML grammar");
        parser.parse(source, None).expect("parse TOML source")
    }

    fn assert_toml_parses(source: &str) {
        let tree = parse_toml(source);
        assert!(
            !tree.root_node().has_error(),
            "expected valid TOML, got parse error for:\n{source}"
        );
    }

    fn assert_toml_rejects(source: &str) {
        let tree = parse_toml(source);
        assert!(
            tree.root_node().has_error(),
            "expected invalid TOML, but parse succeeded for:\n{source}"
        );
    }

    fn parse_syslog(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_syslog::LANGUAGE.into())
            .expect("load Syslog grammar");
        parser.parse(source, None).expect("parse Syslog source")
    }

    fn assert_syslog_parses(source: &str) {
        let tree = parse_syslog(source);
        assert!(
            !tree.root_node().has_error(),
            "expected valid Syslog, got parse error for:\n{source}"
        );
    }

    fn assert_syslog_parses_as_message(source: &str) {
        let tree = parse_syslog(source);
        assert!(
            !tree.root_node().has_error(),
            "expected valid Syslog, got parse error for:\n{source}"
        );
        let first_child = tree
            .root_node()
            .named_child(0)
            .expect("syslog source should produce a named child");
        assert_eq!(first_child.kind(), "syslog_message");
    }

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

    #[test]
    fn gitmodules_queries_include_gitconfig_layers() {
        let config = load_config("gitmodules");
        let queries = load_queries_for_config("gitmodules", &config);

        let highlights = queries.highlights.expect("gitmodules highlights query");
        assert!(highlights.contains("(section_name) @tag"));
        assert!(highlights.contains("shell_command) @error"));
        assert!(highlights.contains("^(~|\\./|\\.\\./|/)"));

        let injections = queries.injections.expect("gitmodules injections query");
        assert!(injections.contains("injection.language \"comment\""));
    }

    #[test]
    fn gitattributes_queries_keep_builtin_objectmode_valid() {
        let queries = load_queries("gitattributes");

        let highlights = queries.highlights.expect("gitattributes highlights query");
        assert!(highlights.contains("builtin_objectmode"));
        assert!(highlights.contains("#not-eq? @error \"builtin_objectmode\""));
    }

    #[test]
    fn gitignore_and_gitconfig_queries_highlight_errors() {
        let gitignore_queries = load_queries("gitignore");
        let gitignore_highlights = gitignore_queries
            .highlights
            .expect("gitignore highlights query");
        assert!(gitignore_highlights.contains("(ERROR) @error"));

        let gitconfig_queries = load_queries("gitconfig");
        let gitconfig_highlights = gitconfig_queries
            .highlights
            .expect("gitconfig highlights query");
        assert!(gitconfig_highlights.contains("(escape_sequence) @escape"));
        assert!(gitconfig_highlights.contains("^(~|\\./|\\.\\./|/)"));
        assert!(gitconfig_highlights.contains("(ERROR) @error"));
    }

    #[test]
    fn gitrebase_queries_highlight_bare_merge_labels() {
        let queries = load_queries("gitrebase");
        let highlights = queries.highlights.expect("gitrebase highlights query");

        assert!(highlights.contains("^(l|label|t|reset|u|update-ref)$"));
        assert!(highlights.contains("(label) @constant\n  (message)? @comment)"));
        assert!(highlights.contains("(label) @constant.builtin\n  (label) @constant"));
    }

    #[test]
    fn syslog_queries_highlight_rfc5424_fields() {
        let queries = load_queries("syslog");
        let highlights = queries.highlights.expect("syslog highlights query");

        assert!(highlights.contains("(sd_id) @tag"));
        assert!(highlights.contains("(param_name) @property"));
        assert!(highlights.contains("(escape_sequence) @string.escape"));
        assert!(highlights.contains("(ERROR) @error"));
    }

    #[test]
    fn syslog_parser_accepts_rfc5424_examples() {
        let first_source = "<34>1 2003-10-11T22:14:15.003Z mymachine.example.com su - ID47 - \u{FEFF}su root failed for lonvick on /dev/pts/8\n";
        assert_syslog_parses_as_message(first_source);

        for source in [
            "<165>1 2003-08-24T05:14:15.000003-07:00 192.0.2.1 myproc 8710 - - %% It's time to make the do-nuts.\n",
            "<165>1 2003-08-24T05:14:15.000003-07:00 host app 123 ID47 [exampleSDID@32473 iut=\"3\" eventSource=\"Application\" eventID=\"1011\"][examplePriority@32473 class=\"high\"] message\n",
            "<34>1 - - - - - -\n",
        ] {
            assert_syslog_parses(source);
        }
    }

    #[test]
    fn toml_config_supports_multiline_strings() {
        let config = load_config("toml");

        assert!(
            config
                .matcher
                .path_suffixes
                .iter()
                .any(|suffix| suffix == "toml")
        );
        assert!(
            config
                .brackets
                .pairs
                .iter()
                .any(|bracket| bracket.start == "\"\"\"" && bracket.end == "\"\"\"")
        );
        assert!(
            config
                .brackets
                .pairs
                .iter()
                .any(|bracket| bracket.start == "'''" && bracket.end == "'''")
        );
    }

    #[test]
    fn toml_queries_include_multiline_string_brackets_and_outline_items() {
        let queries = load_queries("toml");

        let brackets = queries.brackets.expect("toml brackets query");
        assert!(brackets.contains("(\"\\\"\\\"\\\"\" @open"));
        assert!(brackets.contains("'''"));

        let highlights = queries.highlights.expect("toml highlights query");
        assert!(highlights.contains("(escape_sequence) @string.escape"));
        assert!(highlights.contains("(offset_date_time) @string.special"));

        let outline = queries.outline.expect("toml outline query");
        assert!(outline.contains("(table"));
        assert!(outline.contains("(table_array_element"));
        assert!(outline.contains("(pair"));
        assert!(outline.contains("#not-has-parent? @item inline_table"));
    }

    #[test]
    fn toml_parser_accepts_spec_examples() {
        for source in [
            "key = \"value\"\n",
            "physical.color = \"orange\"\nsite.\"google.com\" = true\n",
            "basic = \"I'm a string. \\\"quoted\\\"\\n\"\n",
            "escaped = \"tab\\t esc\\e hex\\x41 unicode\\u03B1 big\\U0001F600\"\n",
            "literal = 'C:\\Users\\nodejs\\templates'\n",
            "multi_basic = \"\"\"\nRoses are red\nViolets are blue\"\"\"\n",
            "multi_literal = '''\nThe first newline is\ntrimmed in literal strings.\n'''\n",
            "int = 0xdead_beef\n",
            "float = 6.626e-34\n",
            "special = -inf\nother = +nan\n",
            "bool = false\n",
            "odt = 1979-05-27T07:32:00Z\n",
            "odt = 1979-05-27 07:32Z\n",
            "ldt = 1979-05-27T07:32:00\n",
            "ldt = 1979-05-27T07:32\n",
            "ld = 1979-05-27\nlt = 07:32:00\n",
            "lt = 07:32\n",
            "values = [1, 2, 3,]\n",
            "contributors = [\n  \"Foo Bar <foo@example.com>\",\n  { name = \"Baz Qux\", email = \"bazqux@example.com\" },\n]\n",
            "[fruit]\napple = \"red\"\n[fruit.apple.texture]\nsmooth = true\n",
            "point = { x = 1, y = 2 }\n",
            "point = { x = 1, y = 2, }\n",
            "[[product]]\nname = \"Hammer\"\n\n[[product]]\nname = \"Nail\"\n",
        ] {
            assert_toml_parses(source);
        }
    }

    #[test]
    fn toml_parser_rejects_spec_invalid_examples() {
        for source in [
            "key =\n",
            "first = \"Tom\" last = \"Preston-Werner\"\n",
            "= \"no key name\"\n",
            "\"\"\"key\"\"\" = \"not allowed\"\n",
            "invalid_float_1 = .7\n",
            "invalid_float_2 = 7.\n",
            "invalid_float_3 = 3.e+20\n",
            "bad_offset = 1979-05-27T07:32:00z\n",
            "bad_delimiter = 1979-05-27t07:32:00\n",
        ] {
            assert_toml_rejects(source);
        }
    }

    #[test]
    fn toml_parser_currently_misses_some_toml_1_1_semantic_constraints() {
        for source in [
            "name = \"Tom\"\nname = \"Pradyun\"\n",
            "fruit.apple = 1\nfruit.apple.smooth = true\n",
            "[fruit]\napple = \"red\"\n[fruit]\norange = \"orange\"\n",
            "[fruit]\napple = \"red\"\n[fruit.apple]\ntexture = \"smooth\"\n",
            "[product]\ntype = { name = \"Nail\" }\ntype.edible = false\n",
            "[product]\ntype.name = \"Nail\"\ntype = { edible = false }\n",
            "fruits = []\n[[fruits]]\nname = \"apple\"\n",
            "[[fruits]]\nname = \"apple\"\n[[fruits.varieties]]\nname = \"red delicious\"\n[fruits.varieties]\nname = \"granny smith\"\n",
        ] {
            assert_toml_parses(source);
        }
    }
}
