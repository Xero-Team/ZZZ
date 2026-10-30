use serde::Deserialize;
use strum::EnumIter;

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum VsCodeTokenScope {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Deserialize)]
pub struct VsCodeTokenColor {
    pub name: Option<String>,
    pub scope: Option<VsCodeTokenScope>,
    pub settings: VsCodeTokenColorSettings,
}

#[derive(Debug, Deserialize)]
pub struct VsCodeTokenColorSettings {
    pub foreground: Option<String>,
    pub background: Option<String>,
    #[serde(rename = "fontStyle")]
    pub font_style: Option<String>,
}

#[derive(Debug, PartialEq, Copy, Clone, EnumIter)]
pub enum ZZZSyntaxToken {
    Attribute,
    Boolean,
    Comment,
    CommentDoc,
    Constant,
    Constructor,
    Embedded,
    Emphasis,
    EmphasisStrong,
    Enum,
    Function,
    Hint,
    Keyword,
    Label,
    LinkText,
    LinkUri,
    Number,
    Operator,
    Predictive,
    Preproc,
    Primary,
    Property,
    Punctuation,
    PunctuationBracket,
    PunctuationDelimiter,
    PunctuationListMarker,
    PunctuationSpecial,
    String,
    StringEscape,
    StringRegex,
    StringSpecial,
    StringSpecialSymbol,
    Tag,
    TextLiteral,
    Title,
    Type,
    Variable,
    VariableSpecial,
    Variant,
}

impl std::fmt::Display for ZZZSyntaxToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ZZZSyntaxToken::Attribute => "attribute",
                ZZZSyntaxToken::Boolean => "boolean",
                ZZZSyntaxToken::Comment => "comment",
                ZZZSyntaxToken::CommentDoc => "comment.doc",
                ZZZSyntaxToken::Constant => "constant",
                ZZZSyntaxToken::Constructor => "constructor",
                ZZZSyntaxToken::Embedded => "embedded",
                ZZZSyntaxToken::Emphasis => "emphasis",
                ZZZSyntaxToken::EmphasisStrong => "emphasis.strong",
                ZZZSyntaxToken::Enum => "enum",
                ZZZSyntaxToken::Function => "function",
                ZZZSyntaxToken::Hint => "hint",
                ZZZSyntaxToken::Keyword => "keyword",
                ZZZSyntaxToken::Label => "label",
                ZZZSyntaxToken::LinkText => "link_text",
                ZZZSyntaxToken::LinkUri => "link_uri",
                ZZZSyntaxToken::Number => "number",
                ZZZSyntaxToken::Operator => "operator",
                ZZZSyntaxToken::Predictive => "predictive",
                ZZZSyntaxToken::Preproc => "preproc",
                ZZZSyntaxToken::Primary => "primary",
                ZZZSyntaxToken::Property => "property",
                ZZZSyntaxToken::Punctuation => "punctuation",
                ZZZSyntaxToken::PunctuationBracket => "punctuation.bracket",
                ZZZSyntaxToken::PunctuationDelimiter => "punctuation.delimiter",
                ZZZSyntaxToken::PunctuationListMarker => "punctuation.list_marker",
                ZZZSyntaxToken::PunctuationSpecial => "punctuation.special",
                ZZZSyntaxToken::String => "string",
                ZZZSyntaxToken::StringEscape => "string.escape",
                ZZZSyntaxToken::StringRegex => "string.regex",
                ZZZSyntaxToken::StringSpecial => "string.special",
                ZZZSyntaxToken::StringSpecialSymbol => "string.special.symbol",
                ZZZSyntaxToken::Tag => "tag",
                ZZZSyntaxToken::TextLiteral => "text.literal",
                ZZZSyntaxToken::Title => "title",
                ZZZSyntaxToken::Type => "type",
                ZZZSyntaxToken::Variable => "variable",
                ZZZSyntaxToken::VariableSpecial => "variable.special",
                ZZZSyntaxToken::Variant => "variant",
            }
        )
    }
}

impl ZZZSyntaxToken {
    pub fn find_best_token_color_match<'a>(
        &self,
        token_colors: &'a [VsCodeTokenColor],
    ) -> Option<&'a VsCodeTokenColor> {
        token_colors
            .iter()
            .filter(|token_color| token_color.settings.foreground.is_some())
            .filter_map(|token_color| {
                let rank = self.rank_match(token_color)?;
                (rank > 0).then_some((rank, token_color))
            })
            // Later entries win ties, matching VS Code's last-wins override
            // semantics.
            .max_by_key(|(rank, _)| *rank)
            .map(|(_, token_color)| token_color)
    }

    fn rank_match(&self, token_color: &VsCodeTokenColor) -> Option<u32> {
        let candidate_scopes = match token_color.scope.as_ref()? {
            VsCodeTokenScope::One(scope) => std::slice::from_ref(scope),
            VsCodeTokenScope::Many(scopes) => scopes.as_slice(),
        }
        .iter()
        .flat_map(|scope| scope.split(',').map(str::trim))
        .collect::<Vec<_>>();

        let scopes_to_match = self.to_vscode();
        let number_of_scopes_to_match = scopes_to_match.len();

        let mut matches = 0;

        for (scope_index, scope) in scopes_to_match.iter().enumerate() {
            // Assign each entry a weight that is inversely proportional to its
            // position in the list.
            //
            // Entries towards the front are weighted higher than those towards the end.
            let weight = (number_of_scopes_to_match - scope_index) as u32;

            if candidate_scopes.contains(scope) {
                matches += 1 + weight;
            }
        }

        Some(matches)
    }

    pub fn fallbacks(&self) -> &[Self] {
        match self {
            ZZZSyntaxToken::CommentDoc => &[ZZZSyntaxToken::Comment],
            ZZZSyntaxToken::Number => &[ZZZSyntaxToken::Constant],
            ZZZSyntaxToken::VariableSpecial => &[ZZZSyntaxToken::Variable],
            ZZZSyntaxToken::PunctuationBracket
            | ZZZSyntaxToken::PunctuationDelimiter
            | ZZZSyntaxToken::PunctuationListMarker
            | ZZZSyntaxToken::PunctuationSpecial => &[ZZZSyntaxToken::Punctuation],
            ZZZSyntaxToken::StringEscape
            | ZZZSyntaxToken::StringRegex
            | ZZZSyntaxToken::StringSpecial
            | ZZZSyntaxToken::StringSpecialSymbol => &[ZZZSyntaxToken::String],
            _ => &[],
        }
    }

    fn to_vscode(self) -> &'static [&'static str] {
        match self {
            ZZZSyntaxToken::Attribute => &["entity.other.attribute-name"],
            ZZZSyntaxToken::Boolean => &["constant.language"],
            ZZZSyntaxToken::Comment => &["comment"],
            ZZZSyntaxToken::CommentDoc => &["comment.block.documentation"],
            ZZZSyntaxToken::Constant => &["constant", "constant.language", "constant.character"],
            ZZZSyntaxToken::Constructor => &[
                "entity.name.tag",
                "entity.name.function.definition.special.constructor",
            ],
            ZZZSyntaxToken::Embedded => &["meta.embedded"],
            ZZZSyntaxToken::Emphasis => &["markup.italic"],
            ZZZSyntaxToken::EmphasisStrong => &[
                "markup.bold",
                "markup.italic markup.bold",
                "markup.bold markup.italic",
            ],
            ZZZSyntaxToken::Enum => &["support.type.enum"],
            ZZZSyntaxToken::Function => &[
                "entity.function",
                "entity.name.function",
                "variable.function",
            ],
            ZZZSyntaxToken::Hint => &[],
            ZZZSyntaxToken::Keyword => &[
                "keyword",
                "keyword.other.fn.rust",
                "keyword.control",
                "keyword.control.fun",
                "keyword.control.class",
                "punctuation.accessor",
                "entity.name.tag",
            ],
            ZZZSyntaxToken::Label => &[
                "label",
                "entity.name",
                "entity.name.import",
                "entity.name.package",
            ],
            ZZZSyntaxToken::LinkText => &["markup.underline.link", "string.other.link"],
            ZZZSyntaxToken::LinkUri => &["markup.underline.link", "string.other.link"],
            ZZZSyntaxToken::Number => &["constant.numeric", "number"],
            ZZZSyntaxToken::Operator => &["operator", "keyword.operator"],
            ZZZSyntaxToken::Predictive => &[],
            ZZZSyntaxToken::Preproc => &[
                "preproc",
                "meta.preprocessor",
                "punctuation.definition.preprocessor",
            ],
            ZZZSyntaxToken::Primary => &[],
            ZZZSyntaxToken::Property => &[
                "variable.member",
                "support.type.property-name",
                "variable.object.property",
                "variable.other.field",
            ],
            ZZZSyntaxToken::Punctuation => &[
                "punctuation",
                "punctuation.section",
                "punctuation.accessor",
                "punctuation.separator",
                "punctuation.definition.tag",
            ],
            ZZZSyntaxToken::PunctuationBracket => &[
                "punctuation.bracket",
                "punctuation.definition.tag.begin",
                "punctuation.definition.tag.end",
            ],
            ZZZSyntaxToken::PunctuationDelimiter => &[
                "punctuation.delimiter",
                "punctuation.separator",
                "punctuation.terminator",
            ],
            ZZZSyntaxToken::PunctuationListMarker => {
                &["markup.list punctuation.definition.list.begin"]
            }
            ZZZSyntaxToken::PunctuationSpecial => &["punctuation.special"],
            ZZZSyntaxToken::String => &["string"],
            ZZZSyntaxToken::StringEscape => {
                &["string.escape", "constant.character", "constant.other"]
            }
            ZZZSyntaxToken::StringRegex => &["string.regex"],
            ZZZSyntaxToken::StringSpecial => &["string.special", "constant.other.symbol"],
            ZZZSyntaxToken::StringSpecialSymbol => {
                &["string.special.symbol", "constant.other.symbol"]
            }
            ZZZSyntaxToken::Tag => &["tag", "entity.name.tag", "meta.tag.sgml"],
            ZZZSyntaxToken::TextLiteral => &["text.literal", "string"],
            ZZZSyntaxToken::Title => &["title", "entity.name"],
            ZZZSyntaxToken::Type => &[
                "entity.name.type",
                "entity.name.type.primitive",
                "entity.name.type.numeric",
                "keyword.type",
                "support.type",
                "support.type.primitive",
                "support.class",
            ],
            ZZZSyntaxToken::Variable => &[
                "variable",
                "variable.language",
                "variable.member",
                "variable.parameter",
                "variable.parameter.function-call",
            ],
            ZZZSyntaxToken::VariableSpecial => &[
                "variable.special",
                "variable.member",
                "variable.annotation",
                "variable.language",
            ],
            ZZZSyntaxToken::Variant => &["variant"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token_color(scope: VsCodeTokenScope, foreground: Option<&str>) -> VsCodeTokenColor {
        VsCodeTokenColor {
            name: None,
            scope: Some(scope),
            settings: VsCodeTokenColorSettings {
                foreground: foreground.map(ToOwned::to_owned),
                background: None,
                font_style: None,
            },
        }
    }

    #[test]
    fn test_find_best_token_color_match_prefers_highest_rank() {
        let token_colors = vec![
            token_color(
                VsCodeTokenScope::One("variable.function".into()),
                Some("#111111"),
            ),
            token_color(
                VsCodeTokenScope::One("entity.function".into()),
                Some("#222222"),
            ),
            token_color(
                VsCodeTokenScope::Many(vec![
                    "entity.name.function, variable.function".into(),
                    "meta.function-call".into(),
                ]),
                Some("#333333"),
            ),
        ];

        let best_match = ZZZSyntaxToken::Function
            .find_best_token_color_match(&token_colors)
            .unwrap();

        assert_eq!(best_match.settings.foreground.as_deref(), Some("#333333"));
    }

    #[test]
    fn test_find_best_token_color_match_prefers_the_later_entry_on_a_tie() {
        let token_colors = vec![
            token_color(
                VsCodeTokenScope::One("entity.name.function".into()),
                Some("#111111"),
            ),
            token_color(
                VsCodeTokenScope::One("entity.name.function".into()),
                Some("#222222"),
            ),
        ];

        let best_match = ZZZSyntaxToken::Function
            .find_best_token_color_match(&token_colors)
            .expect("both entries have the same rank");

        assert_eq!(best_match.settings.foreground.as_deref(), Some("#222222"));
    }

    #[test]
    fn test_find_best_token_color_match_ignores_entries_without_foreground() {
        let token_colors = vec![
            token_color(VsCodeTokenScope::One("entity.name.function".into()), None),
            token_color(
                VsCodeTokenScope::One("variable.function".into()),
                Some("#abcdef"),
            ),
        ];

        let best_match = ZZZSyntaxToken::Function
            .find_best_token_color_match(&token_colors)
            .unwrap();

        assert_eq!(best_match.settings.foreground.as_deref(), Some("#abcdef"));
    }

    #[test]
    fn test_fallbacks_cover_derived_tokens() {
        assert_eq!(
            ZZZSyntaxToken::CommentDoc.fallbacks(),
            &[ZZZSyntaxToken::Comment]
        );
        assert_eq!(
            ZZZSyntaxToken::Number.fallbacks(),
            &[ZZZSyntaxToken::Constant]
        );
        assert_eq!(
            ZZZSyntaxToken::StringEscape.fallbacks(),
            &[ZZZSyntaxToken::String]
        );
        assert!(ZZZSyntaxToken::Keyword.fallbacks().is_empty());
    }
}
