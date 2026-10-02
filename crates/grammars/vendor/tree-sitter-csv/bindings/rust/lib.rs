use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_csv() -> *const ();
    fn tree_sitter_tsv() -> *const ();
    fn tree_sitter_psv() -> *const ();
    fn tree_sitter_semicolon() -> *const ();
}

pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_csv) };
pub const TSV_LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_tsv) };
pub const PSV_LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_psv) };
pub const SEMICOLON_LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_semicolon) };

pub const NODE_TYPES: &str = include_str!("../../src/node-types.json");
pub const TSV_NODE_TYPES: &str = include_str!("../../tsv/src/node-types.json");
pub const PSV_NODE_TYPES: &str = include_str!("../../psv/src/node-types.json");
pub const SEMICOLON_NODE_TYPES: &str = include_str!("../../semicolon/src/node-types.json");

pub const HIGHLIGHTS_QUERY: &str = include_str!("../../queries/highlights.scm");

#[cfg(test)]
mod tests {
    #[test]
    fn test_can_load_grammar() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&super::LANGUAGE.into())
            .expect("Error loading CSV parser");

        parser
            .set_language(&super::TSV_LANGUAGE.into())
            .expect("Error loading TSV parser");

        parser
            .set_language(&super::PSV_LANGUAGE.into())
            .expect("Error loading PSV parser");

        parser
            .set_language(&super::SEMICOLON_LANGUAGE.into())
            .expect("Error loading semicolon parser");
    }
}
