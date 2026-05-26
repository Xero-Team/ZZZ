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
        ("cmd", tree_sitter_cmd::LANGUAGE.into()),
        ("cpp", tree_sitter_cpp::LANGUAGE.into()),
        ("css", tree_sitter_css::LANGUAGE.into()),
        ("csv", tree_sitter_csv::LANGUAGE.into()),
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
        ("powershell", tree_sitter_powershell::LANGUAGE.into()),
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
    use std::path::Path;
    use tree_sitter::Parser;
    use tree_sitter::StreamingIterator;

    fn parse_toml(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_toml::LANGUAGE.into())
            .expect("load TOML grammar");
        parser.parse(source, None).expect("parse TOML source")
    }

    fn parse_cmd(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_cmd::LANGUAGE.into())
            .expect("load CMD grammar");
        parser.parse(source, None).expect("parse CMD source")
    }

    fn parse_csv(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_csv::LANGUAGE.into())
            .expect("load CSV grammar");
        parser.parse(source, None).expect("parse CSV source")
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

    fn assert_cmd_parses(source: &str) {
        let tree = parse_cmd(source);
        assert!(
            !tree.root_node().has_error(),
            "expected valid CMD, got parse error for:\n{source}"
        );
    }

    fn assert_csv_parses(source: &str) {
        let tree = parse_csv(source);
        assert!(
            !tree.root_node().has_error(),
            "expected valid CSV, got parse error for:\n{source}"
        );
    }

    fn cmd_sexp(source: &str) -> String {
        parse_cmd(source).root_node().to_sexp()
    }

    fn read_cmd_testdata(name: &str) -> String {
        std::fs::read_to_string(Path::new("src/cmd/testdata").join(name))
            .unwrap_or_else(|error| panic!("failed to read CMD testdata {name}: {error}"))
    }

    fn read_csv_testdata(name: &str) -> String {
        std::fs::read_to_string(Path::new("src/csv/testdata").join(name))
            .unwrap_or_else(|error| panic!("failed to read CSV testdata {name}: {error}"))
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
    fn cmd_queries_expose_highlights_and_outline() {
        let config = load_config("cmd");
        let queries = load_queries_for_config("cmd", &config);

        let highlights = queries.highlights.expect("cmd highlights query");
        assert!(highlights.contains("(variable_expansion)"));
        assert!(highlights.contains("(set_arithmetic_identifier)"));
        assert!(highlights.contains("(for_f_usebackq_options)"));
        assert!(highlights.contains("(set_variable_name)"));
        assert!(highlights.contains("(copy_path_text)"));
        assert!(highlights.contains("(findstr_literal_option)"));
        assert!(highlights.contains("(move_path_text)"));
        assert!(highlights.contains("(xcopy_exclude_option)"));
        assert!(highlights.contains("(attrib_attribute_change)"));
        assert!(highlights.contains("(mklink_mode)"));
        assert!(highlights.contains("(tree_flag)"));
        assert!(highlights.contains("(chcp_code_page)"));
        assert!(highlights.contains("(subst_delete_flag)"));
        assert!(highlights.contains("(sort_output_flag)"));
        assert!(highlights.contains("(fc_line_buffer_option)"));
        assert!(highlights.contains("(comp_limit_option)"));
        assert!(highlights.contains("(mode_cp_select_option)"));
        assert!(highlights.contains("(mode_baud_option)"));
        assert!(highlights.contains("(doskey_macros_option)"));
        assert!(highlights.contains("(doskey_macro_name)"));
        assert!(highlights.contains("(task_filter_flag)"));
        assert!(highlights.contains("(taskkill_image_flag)"));
        assert!(highlights.contains("(task_format_flag)"));
        assert!(highlights.contains("(remote_system_flag)"));
        assert!(highlights.contains("(shutdown_action_flag)"));
        assert!(highlights.contains("(shutdown_reason_flag)"));
        assert!(highlights.contains("(driverquery_detail_flag)"));
        assert!(highlights.contains("(openfiles_mode_flag)"));
        assert!(highlights.contains("(schtasks_mode_flag)"));
        assert!(highlights.contains("(sc_type_flag)"));
        assert!(highlights.contains("(sc_query_command)"));
        assert!(highlights.contains("(gpresult_scope_flag)"));
        assert!(highlights.contains("(gpresult_html_flag)"));
        assert!(highlights.contains("(bcdedit_store_flag)"));
        assert!(highlights.contains("(bcdedit_command)"));
        assert!(highlights.contains("(compact_exe_option)"));
        assert!(highlights.contains("(replace_flag)"));
        assert!(highlights.contains("(convert_filesystem_flag)"));
        assert!(highlights.contains("(chkdsk_option)"));
        assert!(highlights.contains("(chkntfs_timeout_option)"));
        assert!(highlights.contains("(print_device_option)"));
        assert!(highlights.contains("(icacls_grant_flag)"));
        assert!(highlights.contains("(icacls_restore_flag)"));
        assert!(highlights.contains("(cacls_grant_flag)"));
        assert!(highlights.contains("(format_option)"));
        assert!(highlights.contains("(fsutil_command)"));
        assert!(highlights.contains("(label_mount_flag)"));
        assert!(highlights.contains("(cmd_state_option)"));
        assert!(highlights.contains("(robocopy_flag)"));
        assert!(highlights.contains("(robocopy_copy_flag)"));
        assert!(highlights.contains("(robocopy_log_value)"));
        assert!(highlights.contains("(robocopy_monitor_flag)"));
        assert!(highlights.contains("(robocopy_selection_value)"));
        assert!(highlights.contains("(assoc_extension)"));
        assert!(highlights.contains("(prompt_code)"));
        assert!(highlights.contains("(verify_mode)"));
        assert!(highlights.contains("(setlocal_option)"));
        assert!(highlights.contains("(echo_mode)"));
        assert!(highlights.contains("(color_attribute)"));
        assert!(highlights.contains("(cd_flag)"));
        assert!(highlights.contains("(rmdir_flag)"));
        assert!(highlights.contains("(path_existing_path_reference)"));
        assert!(highlights.contains("(title_text_fragment)"));
        assert!(highlights.contains("(start_cmd_switch)"));
        assert!(highlights.contains("(call_batch_text)"));
        assert!(highlights.contains("(call_label_target"));
        assert!(highlights.contains("(goto_eof_target)"));
        assert!(highlights.contains("(break_statement"));

        let outline = queries.outline.expect("cmd outline query");
        assert!(outline.contains("(label"));
    }

    #[test]
    fn csv_queries_highlight_rfc4180_fields_and_compile() {
        let config = load_config("csv");
        let queries = load_queries_for_config("csv", &config);

        let highlights = queries.highlights.expect("csv highlights query");
        assert!(highlights.contains("@property"));
        assert!(highlights.contains("(delimiter) @punctuation.delimiter"));
        assert!(highlights.contains("(escape_sequence) @string.escape"));
        assert!(highlights.contains("(boolean) @boolean"));
        assert!(highlights.contains("@constant.builtin"));
        assert!(highlights.contains("(null)"));
        assert!(highlights.contains("(na)"));

        let query = tree_sitter::Query::new(&tree_sitter_csv::LANGUAGE.into(), &highlights)
            .expect("compile CSV highlights query");
        let header_capture_index = query
            .capture_names()
            .iter()
            .position(|name| *name == "property")
            .expect("CSV highlights should expose a property capture");

        let source = read_csv_testdata("typed-values.csv");
        let tree = parse_csv(&source);
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut captures = cursor.matches(&query, tree.root_node(), source.as_bytes());
        let mut header_captures = Vec::new();

        while let Some(query_match) = captures.next() {
            for capture in query_match.captures.iter() {
                if capture.index as usize == header_capture_index {
                    header_captures.push(&source[capture.node.byte_range()]);
                }
            }
        }

        assert_eq!(header_captures, vec!["value", "number", "flag", "notes"]);

        let brackets = queries.brackets.expect("csv brackets query");
        tree_sitter::Query::new(&tree_sitter_csv::LANGUAGE.into(), &brackets)
            .expect("compile CSV brackets query");
    }

    #[test]
    fn cmd_parser_handles_control_flow_and_redirection_samples() {
        assert_cmd_parses(
            "@echo off\r\nif exist \"%~1\" (\r\n  echo found %~1\r\n) else (\r\n  echo missing %~1\r\n)\r\nfor /f \"tokens=1,2* delims==\" %%I in ('set') do @echo %%I\r\ncall :label arg1\r\n:label\r\ngoto :eof\r\n",
        );

        assert_cmd_parses(
            "more < input.txt | findstr /i \"needle\" > output.txt\r\nstart \"title\" /b cmd /c \"echo hi && exit /b 0\"\r\nset /a count+=1\r\n",
        );
    }

    #[test]
    fn cmd_parser_distinguishes_if_conditions_for_f_usebackq_and_set_arithmetic_ast() {
        let sexp = cmd_sexp(
            "set /a total=(count + 1) * 2\r\nset /a \"mask = total & 7, flag |= 1\"\r\nif defined PATH echo ok\r\nif cmdextversion 2 echo ok\r\nif /i \"%ERRORLEVEL%\" LEQ 1 goto okay\r\nif not \"%var%\"==\"value\" echo mismatch\r\nfor /f \"tokens=1 delims==\" %%I in ('set') do @echo %%I\r\nfor /f \"usebackq tokens=1\" %%I in (`set`) do @echo %%I\r\nfor /f \"usebackq\" %%I in ('literal string') do @echo %%I\r\nfor /f \"usebackq\" %%I in (\"my file.txt\") do @echo %%I\r\n",
        );

        assert!(sexp.contains("(set_arithmetic_quoted_expression"));
        assert!(sexp.contains("(set_arithmetic_comma_expression"));
        assert!(sexp.contains("(set_arithmetic_bitwise_and_expression"));
        assert!(sexp.contains("(if_defined_condition"));
        assert!(sexp.contains("(if_cmdextversion_condition"));
        assert!(sexp.contains("(if_compare_condition"));
        assert!(sexp.contains("(if_string_condition"));
        assert!(sexp.contains("(for_f_default_statement"));
        assert!(sexp.contains("(for_f_usebackq_statement"));
        assert!(sexp.contains("(for_f_usebackq_command_source"));
        assert!(sexp.contains("(for_f_usebackq_literal_source"));
        assert!(sexp.contains("(for_f_usebackq_file_source"));
    }

    #[test]
    fn cmd_parser_structures_set_start_copy_dir_and_findstr() {
        let sexp = cmd_sexp(
            "set VAR=before\r\nset LIST=\r\nset /p answer=Enter value: \r\nstart \"title\" /d C:\\Temp /b cmd /c \"echo hi\"\r\ncopy /y /a file1.txt+b.txt output.txt /b\r\ndir /a:-d /o:n /t:w *.cmd\r\nfindstr /r /n /c:\"^:label\" script.cmd\r\n",
        );

        assert!(sexp.contains("(set_assignment_clause"));
        assert!(sexp.contains("(set_assignment"));
        assert!(sexp.contains("(set_prompt_clause"));
        assert!(sexp.contains("(set_prompt_assignment"));
        assert!(sexp.contains("(start_statement"));
        assert!(sexp.contains("(start_directory_option"));
        assert!(sexp.contains("(start_command_tail"));
        assert!(sexp.contains("(start_cmd_shell_tail"));
        assert!(sexp.contains("(start_cmd_switch"));
        assert!(sexp.contains("(start_shell_command"));
        assert!(sexp.contains("(copy_statement"));
        assert!(sexp.contains("(copy_source_list"));
        assert!(sexp.contains("(copy_destination"));
        assert!(sexp.contains("(dir_statement"));
        assert!(sexp.contains("(dir_attribute_switch"));
        assert!(sexp.contains("(findstr_statement"));
        assert!(sexp.contains("(findstr_explicit_search_tail"));
        assert!(sexp.contains("(findstr_literal_option"));
    }

    #[test]
    fn cmd_parser_structures_move_xcopy_del_and_rename() {
        let sexp = cmd_sexp(
            "move /y src.txt,dst.txt archive\\\r\nmove olddir newdir\r\nxcopy src dst /d:01-01-2024 /exclude:skip.txt+tmp.txt /compress /-sparse\r\ndel /f /q /a:-h build\\*.obj tmp\\*.tmp\r\nerase /q scratch\\*.tmp\r\nrename old_name.txt new_name.bak\r\n",
        );

        assert!(sexp.contains("(move_statement"));
        assert!(sexp.contains("(move_source_list"));
        assert!(sexp.contains("(move_path_destination"));
        assert!(sexp.contains("(move_directory_destination"));
        assert!(sexp.contains("(xcopy_statement"));
        assert!(sexp.contains("(xcopy_date_option"));
        assert!(sexp.contains("(xcopy_exclude_option"));
        assert!(sexp.contains("(del_statement"));
        assert!(sexp.contains("(del_attribute_switch"));
        assert!(sexp.contains("(rename_statement"));
        assert!(sexp.contains("(rename_source"));
    }

    #[test]
    fn cmd_parser_structures_attrib_more_find_and_mklink() {
        let sexp = cmd_sexp(
            "attrib +r -h archive\\* /s /d /l\r\nmore /e /c /p /s /t4 +10 script.cmd notes.txt\r\nfind /v /n /i /offline \"needle\" script.cmd other.cmd\r\nmklink /d docs-link docs\\source\r\ntype script.cmd | more /e /c\r\n",
        );

        assert!(sexp.contains("(attrib_statement"));
        assert!(sexp.contains("(attrib_attribute_change"));
        assert!(sexp.contains("(attrib_flag"));
        assert!(sexp.contains("(more_file_list_statement"));
        assert!(sexp.contains("(more_stream_statement"));
        assert!(sexp.contains("(more_tabsize_option"));
        assert!(sexp.contains("(more_starting_line"));
        assert!(sexp.contains("(find_statement"));
        assert!(sexp.contains("(find_flag"));
        assert!(sexp.contains("(mklink_statement"));
        assert!(sexp.contains("(mklink_mode"));
    }

    #[test]
    fn cmd_parser_structures_tree_chcp_subst_and_sort() {
        let sexp = cmd_sexp(
            "tree\r\ntree C:\\Windows /f /a\r\nchcp\r\nchcp 65001\r\nsubst\r\nsubst Z: C:\\Tools\r\nsubst Z: /d\r\nsort /r\r\nsort /+3 /m 512 /l C /rec 8192 input.txt /t C:\\Temp /o output.txt\r\n",
        );

        assert!(sexp.contains("(tree_statement"));
        assert!(sexp.contains("(tree_target"));
        assert!(sexp.contains("(tree_flag"));
        assert!(sexp.contains("(chcp_statement"));
        assert!(sexp.contains("(chcp_code_page"));
        assert!(sexp.contains("(subst_statement"));
        assert!(sexp.contains("(subst_assignment"));
        assert!(sexp.contains("(subst_delete"));
        assert!(sexp.contains("(subst_drive"));
        assert!(sexp.contains("(subst_delete_flag"));
        assert!(sexp.contains("(sort_statement"));
        assert!(sexp.contains("(sort_reverse_flag"));
        assert!(sexp.contains("(sort_column_option"));
        assert!(sexp.contains("(sort_memory_option"));
        assert!(sexp.contains("(sort_locale_option"));
        assert!(sexp.contains("(sort_record_option"));
        assert!(sexp.contains("(sort_temp_option"));
        assert!(sexp.contains("(sort_output_option"));
    }

    #[test]
    fn cmd_parser_structures_fc_and_comp() {
        let sexp = cmd_sexp(
            "fc /b first.bin second.bin\r\nfc /a /lb10 /123 first.txt second.txt\r\ncomp left.bin right.bin /d /a /n=10 /c /offline /m\r\n",
        );

        assert!(sexp.contains("(fc_statement"));
        assert!(sexp.contains("(fc_binary_tail"));
        assert!(sexp.contains("(fc_text_tail"));
        assert!(sexp.contains("(fc_binary_flag"));
        assert!(sexp.contains("(fc_text_flag"));
        assert!(sexp.contains("(fc_line_buffer_option"));
        assert!(sexp.contains("(fc_resync_option"));
        assert!(sexp.contains("(fc_file_argument"));
        assert!(sexp.contains("(comp_statement"));
        assert!(sexp.contains("(comp_flag"));
        assert!(sexp.contains("(comp_limit_option"));
        assert!(sexp.contains("(comp_file_argument"));
    }

    #[test]
    fn cmd_parser_structures_mode_and_doskey() {
        let sexp = cmd_sexp(
            "mode\r\nmode con /status\r\nmode lpt1:=com2:\r\nmode con cp select=65001\r\nmode con cp /status\r\nmode con cols=120 lines=40\r\nmode con rate=32 delay=1\r\nmode com1: baud=9600 parity=n data=8 stop=1 to=on xon=off odsr=on octs=off dtr=hs rts=tg idsr=off\r\ndoskey /reinstall /listsize=200 /insert /overstrike /exename=cmd /macrofile=macros.dos /macros /history\r\ndoskey /macros:all\r\ndoskey /macros:cmd\r\ndoskey build=msbuild $*\r\n",
        );

        assert!(sexp.contains("(mode_statement"));
        assert!(sexp.contains("(mode_status_tail"));
        assert!(sexp.contains("(mode_redirect_tail"));
        assert!(sexp.contains("(mode_con_cp_select_tail"));
        assert!(sexp.contains("(mode_con_cp_status_tail"));
        assert!(sexp.contains("(mode_con_display_tail"));
        assert!(sexp.contains("(mode_con_keyboard_tail"));
        assert!(sexp.contains("(mode_serial_tail"));
        assert!(sexp.contains("(mode_cp_select_option"));
        assert!(sexp.contains("(mode_columns_option"));
        assert!(sexp.contains("(mode_rate_option"));
        assert!(sexp.contains("(mode_baud_option"));
        assert!(sexp.contains("(mode_rts_option"));
        assert!(sexp.contains("(doskey_statement"));
        assert!(sexp.contains("(doskey_flag"));
        assert!(sexp.contains("(doskey_listsize_option"));
        assert!(sexp.contains("(doskey_macros_option"));
        assert!(sexp.contains("(doskey_exename_option"));
        assert!(sexp.contains("(doskey_macrofile_option"));
        assert!(sexp.contains("(doskey_macro_definition"));
        assert!(sexp.contains("(doskey_macro_name"));
    }

    #[test]
    fn cmd_parser_structures_tasklist_taskkill_and_systeminfo() {
        let sexp = cmd_sexp(
            "tasklist\r\ntasklist /m\r\ntasklist /s server /u domain\\user /p password /svc /fi \"STATUS eq RUNNING\" /fo csv /nh\r\ntaskkill /im notepad.exe\r\ntaskkill /f /fi \"PID ge 1000\" /im notepad.exe /t\r\nsysteminfo\r\nsysteminfo /s server /u domain\\user /p password /fo csv /nh\r\n",
        );

        assert!(sexp.contains("(tasklist_statement"));
        assert!(sexp.contains("(tasklist_module_option"));
        assert!(sexp.contains("(tasklist_service_flag"));
        assert!(sexp.contains("(remote_connection_clause"));
        assert!(sexp.contains("(task_filter_option"));
        assert!(sexp.contains("(table_list_csv_format_option"));
        assert!(sexp.contains("(no_header_flag"));
        assert!(sexp.contains("(taskkill_statement"));
        assert!(sexp.contains("(taskkill_image_option"));
        assert!(sexp.contains("(taskkill_flag"));
        assert!(sexp.contains("(taskkill_image_flag"));
        assert!(sexp.contains("(systeminfo_statement"));
        assert!(sexp.contains("(remote_system_option"));
        assert!(sexp.contains("(remote_user_option"));
        assert!(sexp.contains("(remote_password_option"));
    }

    #[test]
    fn cmd_parser_structures_shutdown_and_driverquery() {
        let sexp = cmd_sexp(
            "shutdown /?\r\nshutdown /r /m \\\\server /t 60 /d p:2:4 /c \"planned restart\" /f\r\ndriverquery\r\ndriverquery /s server /u domain\\user /p password /fo csv /nh /si\r\n",
        );

        assert!(sexp.contains("(shutdown_statement"));
        assert!(sexp.contains("(shutdown_help_flag"));
        assert!(sexp.contains("(shutdown_action_flag"));
        assert!(sexp.contains("(shutdown_target_option"));
        assert!(sexp.contains("(shutdown_timeout_option"));
        assert!(sexp.contains("(shutdown_reason_option"));
        assert!(sexp.contains("(shutdown_reason_value"));
        assert!(sexp.contains("(shutdown_comment_option"));
        assert!(sexp.contains("(shutdown_aux_flag"));
        assert!(sexp.contains("(driverquery_statement"));
        assert!(sexp.contains("(remote_connection_clause"));
        assert!(sexp.contains("(table_list_csv_format_option"));
        assert!(sexp.contains("(no_header_flag"));
        assert!(sexp.contains("(driverquery_detail_flag"));
    }

    #[test]
    fn cmd_parser_structures_openfiles_schtasks_and_sc() {
        let sexp = cmd_sexp(
            "openfiles /query /?\r\nopenfiles /local /?\r\nschtasks /query /?\r\nschtasks /showsid /?\r\nsc \\\\server query type= service state= all\r\nsc query eventlog\r\nsc start MyService\r\nsc boot ok\r\nsc querylock\r\n",
        );

        assert!(sexp.contains("(openfiles_statement"));
        assert!(sexp.contains("(openfiles_mode_flag"));
        assert!(sexp.contains("(openfiles_help_flag"));
        assert!(sexp.contains("(schtasks_statement"));
        assert!(sexp.contains("(schtasks_mode_flag"));
        assert!(sexp.contains("(schtasks_help_flag"));
        assert!(sexp.contains("(sc_statement"));
        assert!(sexp.contains("(sc_server"));
        assert!(sexp.contains("(sc_query_tail"));
        assert!(sexp.contains("(sc_query_command"));
        assert!(sexp.contains("(sc_type_option"));
        assert!(sexp.contains("(sc_state_option"));
        assert!(sexp.contains("(sc_service_command_tail"));
        assert!(sexp.contains("(sc_service_command"));
        assert!(sexp.contains("(sc_manager_command_tail"));
        assert!(sexp.contains("(sc_boot_command"));
        assert!(sexp.contains("(sc_lock_command"));
    }

    #[test]
    fn cmd_parser_structures_gpresult_and_bcdedit() {
        let sexp = cmd_sexp(
            "gpresult /r\r\ngpresult /s server /u domain\\user /p password /scope computer /user targetuser /z\r\ngpresult /h report.html /f\r\nbcdedit /store C:\\boot\\bcd /enum /v\r\nbcdedit /copy {current} /d \"Cloned entry\"\r\n",
        );

        assert!(sexp.contains("(gpresult_statement"));
        assert!(sexp.contains("(gpresult_mode_option"));
        assert!(sexp.contains("(gpresult_scope_option"));
        assert!(sexp.contains("(gpresult_scope_value"));
        assert!(sexp.contains("(gpresult_target_user_option"));
        assert!(sexp.contains("(gpresult_html_report"));
        assert!(sexp.contains("(gpresult_force_flag"));
        assert!(sexp.contains("(bcdedit_statement"));
        assert!(sexp.contains("(bcdedit_store_option"));
        assert!(sexp.contains("(bcdedit_command_clause"));
        assert!(sexp.contains("(bcdedit_command"));
        assert!(sexp.contains("(bcdedit_identifier"));
        assert!(sexp.contains("(bcdedit_flag"));
    }

    #[test]
    fn cmd_parser_structures_compact_replace_and_convert() {
        let sexp = cmd_sexp(
            "compact\r\ncompact /c /s:C:\\Src /a /i /f /q /exe:lzx file1.txt file2.txt\r\ncompact /compactos:query /windir:C:\\Windows\r\nreplace source.txt C:\\Dest /p /r /s /w /u\r\nconvert C: /fs:ntfs /v /cvtarea:contig.sys /nosecurity /x\r\n",
        );

        assert!(sexp.contains("(compact_statement"));
        assert!(sexp.contains("(compact_general_tail"));
        assert!(sexp.contains("(compact_mode_flag"));
        assert!(sexp.contains("(compact_recursive_option"));
        assert!(sexp.contains("(compact_exe_option"));
        assert!(sexp.contains("(compact_target_argument"));
        assert!(sexp.contains("(compact_os_tail"));
        assert!(sexp.contains("(compact_os_option"));
        assert!(sexp.contains("(compact_windir_option"));
        assert!(sexp.contains("(replace_statement"));
        assert!(sexp.contains("(replace_source_argument"));
        assert!(sexp.contains("(replace_destination_argument"));
        assert!(sexp.contains("(replace_flag"));
        assert!(sexp.contains("(convert_statement"));
        assert!(sexp.contains("(convert_volume_argument"));
        assert!(sexp.contains("(convert_filesystem_flag"));
        assert!(sexp.contains("(convert_option"));
    }

    #[test]
    fn cmd_parser_structures_chkdsk_chkntfs_and_print() {
        let sexp = cmd_sexp(
            "chkdsk C: /f /r /x /l:4096 /scan /perf\r\nchkntfs /d\r\nchkntfs /t:30\r\nchkntfs /x C: D:\r\nchkntfs /c C:\r\nprint /d:lpt1 report.txt notes.txt\r\n",
        );

        assert!(sexp.contains("(chkdsk_statement"));
        assert!(sexp.contains("(chkdsk_target_argument"));
        assert!(sexp.contains("(chkdsk_option"));
        assert!(sexp.contains("(chkntfs_statement"));
        assert!(sexp.contains("(chkntfs_default_flag"));
        assert!(sexp.contains("(chkntfs_timeout_option"));
        assert!(sexp.contains("(chkntfs_exclude_clause"));
        assert!(sexp.contains("(chkntfs_schedule_clause"));
        assert!(sexp.contains("(chkntfs_volume_argument"));
        assert!(sexp.contains("(print_statement"));
        assert!(sexp.contains("(print_device_option"));
        assert!(sexp.contains("(print_file_argument"));
    }

    #[test]
    fn cmd_parser_structures_icacls_cacls_and_recover() {
        let sexp = cmd_sexp(
            "icacls C:\\Temp\\file.txt /grant:r User:F /t /c /l /q\r\nicacls C:\\Temp /restore acl.txt /c /l /q\r\ncacls file.txt /e /g user:f /c\r\nrecover C:\\Temp\\broken.txt\r\n",
        );

        assert!(sexp.contains("(icacls_statement"));
        assert!(sexp.contains("(icacls_edit_tail"));
        assert!(sexp.contains("(icacls_grant_action"));
        assert!(sexp.contains("(icacls_grant_flag"));
        assert!(sexp.contains("(icacls_common_flag"));
        assert!(sexp.contains("(icacls_restore_tail"));
        assert!(sexp.contains("(icacls_restore_flag"));
        assert!(sexp.contains("(cacls_statement"));
        assert!(sexp.contains("(cacls_simple_flag"));
        assert!(sexp.contains("(cacls_grant_option"));
        assert!(sexp.contains("(cacls_grant_flag"));
        assert!(sexp.contains("(recover_statement"));
        assert!(sexp.contains("(recover_target_argument"));
    }

    #[test]
    fn cmd_parser_structures_format_and_fsutil() {
        let sexp = cmd_sexp(
            "format C: /fs:ntfs /v:DATA /q /a:4096 /x /p:1 /s:enable\r\nfsutil behavior\r\nfsutil file queryCaseSensitiveInfo C:\\Temp\\demo.txt\r\n",
        );

        assert!(sexp.contains("(format_statement"));
        assert!(sexp.contains("(format_volume_argument"));
        assert!(sexp.contains("(format_option"));
        assert!(sexp.contains("(fsutil_statement"));
        assert!(sexp.contains("(fsutil_command"));
        assert!(sexp.contains("(fsutil_argument"));
    }

    #[test]
    fn cmd_parser_structures_label_and_cmd() {
        let sexp = cmd_sexp(
            "label C: WORK\r\nlabel /mp C:\\Mount DATA\r\ncmd /q /d /v:on /c \"echo hi && exit /b 0\"\r\ncmd /a /e:off /f:on /k dir\r\n",
        );

        assert!(sexp.contains("(label_statement"));
        assert!(sexp.contains("(label_drive_tail"));
        assert!(sexp.contains("(label_mount_tail"));
        assert!(sexp.contains("(label_mount_flag"));
        assert!(sexp.contains("(label_drive"));
        assert!(sexp.contains("(label_value_argument"));
        assert!(sexp.contains("(cmd_statement"));
        assert!(sexp.contains("(cmd_flag"));
        assert!(sexp.contains("(cmd_state_option"));
        assert!(sexp.contains("(cmd_command_tail"));
        assert!(sexp.contains("(cmd_shell_mode"));
    }

    #[test]
    fn cmd_parser_structures_robocopy() {
        let sexp = cmd_sexp(
            "robocopy C:\\Src D:\\Dst *.txt *.md /e /copy:DAT /r:2 /w:5 /log:copy.log\r\nrobocopy \\\\server\\share C:\\Backup /mir /xd node_modules dist /xf *.tmp *.bak\r\nrobocopy C:\\Src D:\\Dst *.txt /copy:DAT /dcopy:DA /ia:RASH /xa:SH /r:2 /w:5 /log:copy.log /job:nightly /save:weekly\r\nrobocopy C:\\Src D:\\Dst *.txt /lev:2 /mon:5 /mot:10 /rh:0100-0500 /ipg:8 /mt:16 /iomaxsize:1M /iorate:10M /threshold:512K /max:1024 /min:10 /maxage:30 /minage:2 /maxlad:30 /minlad:2 /r:3 /w:7 /lfsm:1G\r\n",
        );

        assert!(sexp.contains("(robocopy_statement"));
        assert!(sexp.contains("(robocopy_source_argument"));
        assert!(sexp.contains("(robocopy_destination_argument"));
        assert!(sexp.contains("(robocopy_file_argument"));
        assert!(sexp.contains("(robocopy_flag"));
        assert!(sexp.contains("(robocopy_copy_option"));
        assert!(sexp.contains("(robocopy_copy_flag"));
        assert!(sexp.contains("(robocopy_copy_value"));
        assert!(sexp.contains("(robocopy_attribute_option"));
        assert!(sexp.contains("(robocopy_attribute_flag"));
        assert!(sexp.contains("(robocopy_retry_option"));
        assert!(sexp.contains("(robocopy_retry_flag"));
        assert!(sexp.contains("(robocopy_log_option"));
        assert!(sexp.contains("(robocopy_log_flag"));
        assert!(sexp.contains("(robocopy_job_option"));
        assert!(sexp.contains("(robocopy_job_flag"));
        assert!(sexp.contains("(robocopy_monitor_option"));
        assert!(sexp.contains("(robocopy_monitor_flag"));
        assert!(sexp.contains("(robocopy_throttle_option"));
        assert!(sexp.contains("(robocopy_throttle_flag"));
        assert!(sexp.contains("(robocopy_selection_option"));
        assert!(sexp.contains("(robocopy_selection_flag"));
        assert!(sexp.contains("(robocopy_list_option"));
        assert!(sexp.contains("(robocopy_list_flag"));
    }

    #[test]
    fn cmd_parser_structures_assoc_ftype_path_and_prompt() {
        let sexp = cmd_sexp(
            "assoc .txt=txtfile\r\nassoc .log\r\nassoc .bak=\r\nftype txtfile=\"%SystemRoot%\\System32\\NOTEPAD.EXE\" \"%1\"\r\nftype txtfile=\r\npath\r\npath C:\\Windows;C:\\Tools;%PATH%\r\npath ;\r\nprompt $p$g$+$m\r\n",
        );

        assert!(sexp.contains("(assoc_statement"));
        assert!(sexp.contains("(assoc_assignment"));
        assert!(sexp.contains("(assoc_query"));
        assert!(sexp.contains("(ftype_statement"));
        assert!(sexp.contains("(ftype_assignment"));
        assert!(sexp.contains("(ftype_command"));
        assert!(sexp.contains("(path_statement"));
        assert!(sexp.contains("(path_assignment"));
        assert!(sexp.contains("(path_directory_entry"));
        assert!(sexp.contains("(path_existing_path_reference"));
        assert!(sexp.contains("(path_separator"));
        assert!(sexp.contains("(path_clear"));
        assert!(sexp.contains("(prompt_statement"));
        assert!(sexp.contains("(prompt_code"));
    }

    #[test]
    fn cmd_parser_structures_setlocal_help_type_and_echo() {
        let sexp = cmd_sexp(
            "setlocal enableextensions enabledelayedexpansion\r\nendlocal\r\nhelp dir\r\ntype script.cmd notes.txt\r\necho off\r\necho Build started\r\necho\r\n",
        );

        assert!(sexp.contains("(setlocal_statement"));
        assert!(sexp.contains("(setlocal_option"));
        assert!(sexp.contains("(endlocal_statement"));
        assert!(sexp.contains("(help_statement"));
        assert!(sexp.contains("(help_topic"));
        assert!(sexp.contains("(type_statement"));
        assert!(sexp.contains("(type_file_argument"));
        assert!(sexp.contains("(echo_statement"));
        assert!(sexp.contains("(echo_mode_clause"));
        assert!(sexp.contains("(echo_message_clause"));
    }

    #[test]
    fn cmd_parser_structures_verify_vol_title_color_and_datetime() {
        let sexp = cmd_sexp(
            "verify on\r\nverify\r\nvol C:\r\nvol\r\ntitle Build %COMPUTERNAME% Console\r\ncolor 0a\r\ncolor\r\ndate /t\r\ndate 2026-05-24\r\ntime 12:34:56.78\r\n",
        );

        assert!(sexp.contains("(verify_statement"));
        assert!(sexp.contains("(verify_mode"));
        assert!(sexp.contains("(vol_statement"));
        assert!(sexp.contains("(vol_drive"));
        assert!(sexp.contains("(title_statement"));
        assert!(sexp.contains("(title_assignment"));
        assert!(sexp.contains("(title_text_fragment"));
        assert!(sexp.contains("(title_variable_expansion"));
        assert!(sexp.contains("(color_statement"));
        assert!(sexp.contains("(color_attribute"));
        assert!(sexp.contains("(date_statement"));
        assert!(sexp.contains("(date_time_flag"));
        assert!(sexp.contains("(date_time_number"));
        assert!(sexp.contains("(date_separator"));
        assert!(sexp.contains("(time_statement"));
        assert!(sexp.contains("(time_value"));
    }

    #[test]
    fn cmd_parser_structures_directory_builtins() {
        let sexp = cmd_sexp(
            "cd /d C:\\Temp\r\ncd ..\r\ncd C:\r\npushd ..\\scripts\r\npopd\r\nmkdir build\\output\r\nrmdir /s /q build\\output\r\n",
        );

        assert!(sexp.contains("(cd_statement"));
        assert!(sexp.contains("(cd_flag"));
        assert!(sexp.contains("(cd_parent"));
        assert!(sexp.contains("(cd_drive_query"));
        assert!(sexp.contains("(pushd_statement"));
        assert!(sexp.contains("(popd_statement"));
        assert!(sexp.contains("(mkdir_statement"));
        assert!(sexp.contains("(rmdir_statement"));
        assert!(sexp.contains("(rmdir_flag"));
    }

    #[test]
    fn cmd_parser_structures_call_goto_and_zero_arg_builtins() {
        let sexp = cmd_sexp(
            "call build.cmd one two\r\ncall :done one two\r\ngoto :eof\r\ngoto :done\r\npause\r\ncls\r\nver\r\nbreak\r\n:done\r\n",
        );

        assert!(sexp.contains("(call_batch_statement"));
        assert!(sexp.contains("(call_batch_target"));
        assert!(sexp.contains("(call_batch_argument"));
        assert!(sexp.contains("(call_label_statement"));
        assert!(sexp.contains("(call_label_target"));
        assert!(sexp.contains("(call_label_argument"));
        assert!(sexp.contains("(goto_eof_statement"));
        assert!(sexp.contains("(goto_eof_target"));
        assert!(sexp.contains("(goto_label_statement"));
        assert!(sexp.contains("(goto_label_target"));
        assert!(sexp.contains("(pause_statement"));
        assert!(sexp.contains("(cls_statement"));
        assert!(sexp.contains("(ver_statement"));
        assert!(sexp.contains("(break_statement"));
    }

    #[test]
    fn cmd_parser_accepts_help_derived_cmd_testdata() {
        assert_cmd_parses(&read_cmd_testdata("help-derived-builtins.cmd"));
        assert_cmd_parses(&read_cmd_testdata("findstr-default-heuristics.cmd"));

        let sexp = cmd_sexp(&read_cmd_testdata("findstr-default-heuristics.cmd"));
        assert!(sexp.contains("(findstr_default_search_tail"));
        assert!(sexp.contains("file: (findstr_default_file_argument"));

        let builtins_sexp = cmd_sexp(&read_cmd_testdata("help-derived-builtins.cmd"));
        assert!(builtins_sexp.contains("(attrib_statement"));
        assert!(builtins_sexp.contains("(assoc_statement"));
        assert!(builtins_sexp.contains("(ftype_statement"));
        assert!(builtins_sexp.contains("(cd_statement"));
        assert!(builtins_sexp.contains("(pushd_statement"));
        assert!(builtins_sexp.contains("(popd_statement"));
        assert!(builtins_sexp.contains("(mkdir_statement"));
        assert!(builtins_sexp.contains("(rmdir_statement"));
        assert!(builtins_sexp.contains("(more_file_list_statement"));
        assert!(builtins_sexp.contains("(more_stream_statement"));
        assert!(builtins_sexp.contains("(find_statement"));
        assert!(builtins_sexp.contains("(mklink_statement"));
        assert!(builtins_sexp.contains("(tree_statement"));
        assert!(builtins_sexp.contains("(chcp_statement"));
        assert!(builtins_sexp.contains("(subst_statement"));
        assert!(builtins_sexp.contains("(sort_statement"));
        assert!(builtins_sexp.contains("(fc_statement"));
        assert!(builtins_sexp.contains("(comp_statement"));
        assert!(builtins_sexp.contains("(mode_statement"));
        assert!(builtins_sexp.contains("(doskey_statement"));
        assert!(builtins_sexp.contains("(tasklist_statement"));
        assert!(builtins_sexp.contains("(taskkill_statement"));
        assert!(builtins_sexp.contains("(systeminfo_statement"));
        assert!(builtins_sexp.contains("(shutdown_statement"));
        assert!(builtins_sexp.contains("(driverquery_statement"));
        assert!(builtins_sexp.contains("(openfiles_statement"));
        assert!(builtins_sexp.contains("(schtasks_statement"));
        assert!(builtins_sexp.contains("(sc_statement"));
        assert!(builtins_sexp.contains("(gpresult_statement"));
        assert!(builtins_sexp.contains("(bcdedit_statement"));
        assert!(builtins_sexp.contains("(compact_statement"));
        assert!(builtins_sexp.contains("(replace_statement"));
        assert!(builtins_sexp.contains("(convert_statement"));
        assert!(builtins_sexp.contains("(chkdsk_statement"));
        assert!(builtins_sexp.contains("(chkntfs_statement"));
        assert!(builtins_sexp.contains("(print_statement"));
        assert!(builtins_sexp.contains("(icacls_statement"));
        assert!(builtins_sexp.contains("(cacls_statement"));
        assert!(builtins_sexp.contains("(recover_statement"));
        assert!(builtins_sexp.contains("(format_statement"));
        assert!(builtins_sexp.contains("(fsutil_statement"));
        assert!(builtins_sexp.contains("(label_statement"));
        assert!(builtins_sexp.contains("(cmd_statement"));
        assert!(builtins_sexp.contains("(robocopy_statement"));
        assert!(builtins_sexp.contains("(path_statement"));
        assert!(builtins_sexp.contains("(prompt_statement"));
        assert!(builtins_sexp.contains("(setlocal_statement"));
        assert!(builtins_sexp.contains("(endlocal_statement"));
        assert!(builtins_sexp.contains("(help_statement"));
        assert!(builtins_sexp.contains("(type_statement"));
        assert!(builtins_sexp.contains("(echo_statement"));
        assert!(builtins_sexp.contains("(call_batch_statement"));
        assert!(builtins_sexp.contains("(call_label_statement"));
        assert!(builtins_sexp.contains("(goto_eof_statement"));
        assert!(builtins_sexp.contains("(goto_label_statement"));
        assert!(builtins_sexp.contains("(pause_statement"));
        assert!(builtins_sexp.contains("(cls_statement"));
        assert!(builtins_sexp.contains("(ver_statement"));
        assert!(builtins_sexp.contains("(break_statement"));
        assert!(builtins_sexp.contains("(verify_statement"));
        assert!(builtins_sexp.contains("(vol_statement"));
        assert!(builtins_sexp.contains("(title_statement"));
        assert!(builtins_sexp.contains("(color_statement"));
        assert!(builtins_sexp.contains("(date_statement"));
        assert!(builtins_sexp.contains("(time_statement"));
    }

    #[test]
    fn cmd_parser_accepts_help_derived_builtin_invocations() {
        assert_cmd_parses(
            "@echo off\r\nsetlocal enabledelayedexpansion\r\nassoc .txt=txtfile\r\nftype txtfile=\"%SystemRoot%\\System32\\NOTEPAD.EXE\" \"%1\"\r\ncd /d C:\\Temp\r\ndir /a:-d /b /s *.cmd\r\npushd ..\\scripts\r\npopd\r\ncopy /y source.txt destination.txt\r\ncopy /a file1.txt+file2.txt output.txt\r\nmove /y *.txt archive\\\r\ndel /q *.tmp\r\nmkdir build\\output\r\nrmdir /s /q build\\output\r\ntype script.cmd\r\nmore /e +10 < script.cmd\r\nfind /i \"needle\" file.txt\r\nfindstr /r /n \"^:label\" script.cmd\r\npath C:\\Windows;C:\\Tools\r\nprompt $p$g\r\nverify on\r\nver\r\nvol C:\r\nren old.txt new.txt\r\npause\r\ncls\r\ncall setup.cmd one two\r\ncall :label one two\r\n:label\r\nshift /2\r\nendlocal\r\ngoto :eof\r\nexit /b 1\r\n",
        );
    }

    #[test]
    fn cmd_parser_covers_all_per_command_testdata_files() {
        // Each entry maps the file stem to the S-expression node kind that MUST appear in the parse
        // result. Aliases (chdir, erase, md, rd, ren) map to the same node as their canonical form.
        // wmic is intentionally left as simple_command (help text says "not a recognized command").
        let expected_nodes: &[(&str, &str)] = &[
            ("assoc", "(assoc_statement"),
            ("attrib", "(attrib_statement"),
            ("bcdedit", "(bcdedit_statement"),
            ("break", "(break_statement"),
            ("cacls", "(cacls_statement"),
            ("call", "(call_batch_statement"),
            ("cd", "(cd_statement"),
            ("chcp", "(chcp_statement"),
            ("chdir", "(cd_statement"),
            ("chkdsk", "(chkdsk_statement"),
            ("chkntfs", "(chkntfs_statement"),
            ("cls", "(cls_statement"),
            ("cmd", "(cmd_statement"),
            ("color", "(color_statement"),
            ("comp", "(comp_statement"),
            ("compact", "(compact_statement"),
            ("convert", "(convert_statement"),
            ("copy", "(copy_statement"),
            ("date", "(date_statement"),
            ("del", "(del_statement"),
            ("dir", "(dir_statement"),
            ("doskey", "(doskey_statement"),
            ("driverquery", "(driverquery_statement"),
            ("echo", "(echo_statement"),
            ("endlocal", "(endlocal_statement"),
            ("erase", "(del_statement"),
            ("exit", "(exit_statement"),
            ("fc", "(fc_statement"),
            ("find", "(find_statement"),
            ("findstr", "(findstr_statement"),
            ("for", "(for_"),
            ("format", "(format_statement"),
            ("fsutil", "(fsutil_statement"),
            ("ftype", "(ftype_statement"),
            ("goto", "(goto_"),
            ("gpresult", "(gpresult_statement"),
            ("help", "(help_statement"),
            ("icacls", "(icacls_statement"),
            ("if", "(if_statement"),
            ("label", "(label_statement"),
            ("md", "(mkdir_statement"),
            ("mkdir", "(mkdir_statement"),
            ("mklink", "(mklink_statement"),
            ("mode", "(mode_statement"),
            ("more", "(more_"),
            ("move", "(move_statement"),
            ("openfiles", "(openfiles_statement"),
            ("path", "(path_statement"),
            ("pause", "(pause_statement"),
            ("popd", "(popd_statement"),
            ("print", "(print_statement"),
            ("prompt", "(prompt_statement"),
            ("pushd", "(pushd_statement"),
            ("rd", "(rmdir_statement"),
            ("recover", "(recover_statement"),
            ("rem", "(comment"),
            ("ren", "(rename_statement"),
            ("rename", "(rename_statement"),
            ("replace", "(replace_statement"),
            ("rmdir", "(rmdir_statement"),
            ("robocopy", "(robocopy_statement"),
            ("sc", "(sc_statement"),
            ("schtasks", "(schtasks_statement"),
            ("set", "(set_statement"),
            ("setlocal", "(setlocal_statement"),
            ("shift", "(shift_statement"),
            ("shutdown", "(shutdown_statement"),
            ("sort", "(sort_statement"),
            ("start", "(start_statement"),
            ("subst", "(subst_statement"),
            ("systeminfo", "(systeminfo_statement"),
            ("taskkill", "(taskkill_statement"),
            ("tasklist", "(tasklist_statement"),
            ("time", "(time_statement"),
            ("title", "(title_statement"),
            ("tree", "(tree_statement"),
            ("type", "(type_statement"),
            ("ver", "(ver_statement"),
            ("verify", "(verify_statement"),
            ("vol", "(vol_statement"),
            ("wmic", "(simple_command"),
            ("xcopy", "(xcopy_statement"),
        ];

        let dir = Path::new("src/cmd/testdata/help-derived-commands");
        for (stem, expected_node) in expected_nodes {
            let filename = format!("{stem}.cmd");
            let source = std::fs::read_to_string(dir.join(&filename))
                .unwrap_or_else(|e| panic!("missing testdata file {filename}: {e}"));

            let tree = parse_cmd(&source);
            assert!(
                !tree.root_node().has_error(),
                "parse error in {filename}:\n{source}"
            );

            let sexp = tree.root_node().to_sexp();
            assert!(
                sexp.contains(expected_node),
                "expected {expected_node} in sexp for {filename}, got:\n{sexp}"
            );
        }
    }

    #[test]
    fn gitrebase_queries_highlight_bare_merge_labels() {
        let queries = load_queries("gitrebase");
        let highlights = queries.highlights.expect("gitrebase highlights query");

        assert!(highlights.contains("^(l|label|t|reset|u|update-ref)$"));
        assert!(highlights.contains("(((command) @function"));
        assert!(highlights.contains("(label) @constant"));
        assert!(highlights.contains("(message)? @comment"));
        assert!(highlights.contains("(label) @constant.builtin"));
        assert!(highlights.contains("^(m|merge)$"));
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
    fn csv_parser_accepts_testdata_examples() {
        assert_csv_parses(&read_csv_testdata("rfc4180.csv"));
        assert_csv_parses(&read_csv_testdata("typed-values.csv"));
        assert_csv_parses(&read_csv_testdata("edge-cases.csv"));
        assert_csv_parses(&read_csv_testdata("crlf.csv"));

        let sexp = parse_csv(&read_csv_testdata("typed-values.csv"))
            .root_node()
            .to_sexp();
        assert!(sexp.contains("(null)"));
        assert!(sexp.contains("(hex)"));
        assert!(sexp.contains("(boolean)"));
        assert!(sexp.contains("(na)"));
        assert!(sexp.contains("(float)"));
    }

    #[test]
    fn csv_parser_accepts_rfc4180_boundary_cases() {
        let edge_source = read_csv_testdata("edge-cases.csv");
        assert_csv_parses(&edge_source);

        let edge_tree = parse_csv(&edge_source);
        assert_eq!(edge_tree.root_node().named_child_count(), 4);
        let edge_sexp = edge_tree.root_node().to_sexp();
        assert!(edge_sexp.contains("(record"));
        assert!(edge_sexp.contains("sep: (delimiter)"));
        assert!(edge_sexp.contains("(string (non_escaped))"));

        let crlf_source = read_csv_testdata("crlf.csv");
        assert!(crlf_source.contains("\r\n"));
        assert_csv_parses(&crlf_source);

        assert_csv_parses("header1,header2\r\nvalue1,value2");
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
