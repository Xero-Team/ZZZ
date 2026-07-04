use proc_macro::{Delimiter, Span, TokenStream, TokenTree};
use syn::Error;

#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
static SQLITE: std::sync::LazyLock<sqlez::thread_safe_connection::ThreadSafeConnection> =
    std::sync::LazyLock::new(|| {
        sqlez::thread_safe_connection::ThreadSafeConnection::new(
            ":memory:",
            false,
            None,
            Some(sqlez::thread_safe_connection::locking_queue()),
        )
    });

#[proc_macro]
pub fn sql(tokens: TokenStream) -> TokenStream {
    let (spans, sql) = make_sql(tokens);

    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    let error = SQLITE.sql_has_syntax_error(sql.trim());

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    let error: Option<(String, usize)> = None;

    let formatted_sql = repair_sqlite_numbered_placeholders(sqlformat::format(
        &sql,
        &sqlformat::QueryParams::None,
        &Default::default(),
    ));

    if let Some((error, error_offset)) = error {
        create_error(spans, error_offset, error, &formatted_sql)
    } else {
        format!("r#\"{}\"#", &formatted_sql).parse().unwrap()
    }
}

fn repair_sqlite_numbered_placeholders(formatted_sql: String) -> String {
    let mut repaired_sql = String::with_capacity(formatted_sql.len());
    let mut chars = formatted_sql.chars().peekable();

    while let Some(character) = chars.next() {
        if character == '?' {
            let mut spaces = 0;
            while matches!(chars.peek(), Some(' ')) {
                chars.next();
                spaces += 1;
            }

            let mut digits = String::new();
            while let Some(next_character) = chars.peek() {
                if next_character.is_ascii_digit() {
                    digits.push(*next_character);
                    chars.next();
                } else {
                    break;
                }
            }

            repaired_sql.push('?');
            if digits.is_empty() {
                for _ in 0..spaces {
                    repaired_sql.push(' ');
                }
            } else {
                repaired_sql.push_str(&digits);
            }
            continue;
        }

        repaired_sql.push(character);
    }

    repaired_sql
}

fn create_error(
    spans: Vec<(usize, Span)>,
    error_offset: usize,
    error: String,
    formatted_sql: &String,
) -> TokenStream {
    let error_span = spans
        .into_iter()
        .skip_while(|(offset, _)| offset <= &error_offset)
        .map(|(_, span)| span)
        .next()
        .unwrap_or_else(Span::call_site);
    let error_text = format!("Sql Error: {}\nFor Query: {}", error, formatted_sql);
    TokenStream::from(Error::new(error_span.into(), error_text).into_compile_error())
}

fn make_sql(tokens: TokenStream) -> (Vec<(usize, Span)>, String) {
    let mut sql_tokens = vec![];
    flatten_stream(tokens, &mut sql_tokens);
    // Lookup of spans by offset at the end of the token
    let mut spans: Vec<(usize, Span)> = Vec::new();
    let mut sql = String::new();
    for (token_text, span) in sql_tokens {
        sql.push_str(&token_text);
        spans.push((sql.len(), span));
    }
    (spans, sql)
}

/// This method exists to normalize the representation of groups
/// to always include spaces between tokens. This is why we don't use the usual .to_string().
/// This allows our token search in token_at_offset to resolve
/// ambiguity of '(tokens)' vs. '( token )', due to sqlite requiring byte offsets
fn flatten_stream(tokens: TokenStream, result: &mut Vec<(String, Span)>) {
    for token_tree in tokens.into_iter() {
        match token_tree {
            TokenTree::Group(group) => {
                // push open delimiter
                result.push((open_delimiter(group.delimiter()), group.span()));
                // recurse
                flatten_stream(group.stream(), result);
                // push close delimiter
                result.push((close_delimiter(group.delimiter()), group.span()));
            }
            TokenTree::Ident(ident) => {
                result.push((format!("{} ", ident), ident.span()));
            }
            leaf_tree => result.push((leaf_tree.to_string(), leaf_tree.span())),
        }
    }
}

fn open_delimiter(delimiter: Delimiter) -> String {
    match delimiter {
        Delimiter::Parenthesis => "( ".to_owned(),
        Delimiter::Brace => "{ ".to_owned(),
        Delimiter::Bracket => "[ ".to_owned(),
        Delimiter::None => "".to_owned(),
    }
}

fn close_delimiter(delimiter: Delimiter) -> String {
    match delimiter {
        Delimiter::Parenthesis => " ) ".to_owned(),
        Delimiter::Brace => " } ".to_owned(),
        Delimiter::Bracket => " ] ".to_owned(),
        Delimiter::None => "".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repair_sqlite_numbered_placeholders_compacts_digit_suffixes() {
        let sql = "SELECT ? 1, ?, ?   23, ?   FROM test".to_owned();

        assert_eq!(
            repair_sqlite_numbered_placeholders(sql),
            "SELECT ?1, ?, ?23, ?   FROM test"
        );
    }

    #[test]
    fn delimiter_mappings_match_token_delimiters() {
        assert_eq!(open_delimiter(Delimiter::Parenthesis), "( ");
        assert_eq!(close_delimiter(Delimiter::Parenthesis), " ) ");
        assert_eq!(open_delimiter(Delimiter::Brace), "{ ");
        assert_eq!(close_delimiter(Delimiter::Brace), " } ");
        assert_eq!(open_delimiter(Delimiter::Bracket), "[ ");
        assert_eq!(close_delimiter(Delimiter::Bracket), " ] ");
        assert_eq!(open_delimiter(Delimiter::None), "");
        assert_eq!(close_delimiter(Delimiter::None), "");
    }
}
