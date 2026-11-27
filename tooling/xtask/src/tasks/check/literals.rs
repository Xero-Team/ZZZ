//! Blanks out the inside of string and char literals before scanning source.
//!
//! A source scan that looks for attributes or keywords reads prose and test
//! fixtures as code unless the literals are removed first. That is not a
//! hypothetical: the suppression gate's own error message mentions `#[allow]`
//! and its tests build attributes as string literals, so a scanner without this
//! step flags itself.
//!
//! [`mask`] keeps the input's byte length, so a caller can find a match in the
//! masked text and use that offset to slice the original.

/// Masks the contents of every string, raw string, and char literal.
///
/// The delimiters stay, because a scan over the masked text still has to see
/// where the literal was. Each masked byte becomes a space, so the result has
/// the input's length and stays valid UTF-8.
///
/// Lifetimes are left alone: a `'` only opens a char literal when one closes
/// within a few bytes, which is what separates `'\n'` and `'\''` from `&'a str`.
pub fn mask(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut masked = bytes.to_vec();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'"' {
            index = mask_quoted(bytes, index + 1, &mut masked);
            continue;
        }
        if let Some((content_start, hashes)) = raw_literal_start(bytes, index) {
            index = mask_raw(bytes, content_start, hashes, &mut masked);
            continue;
        }
        if let Some((content_start, content_end)) = char_literal_contents(source, index) {
            masked[content_start..content_end].fill(b' ');
            index = content_end + 1;
            continue;
        }
        index += source[index..].chars().next().map_or(1, char::len_utf8);
    }

    String::from_utf8(masked)
        .expect("masking only replaces literal contents with spaces, which cannot break UTF-8")
}

/// Masks a `"` literal that starts at `from`, returning the index after its
/// closing quote or the end of input for an unterminated one.
fn mask_quoted(bytes: &[u8], from: usize, masked: &mut [u8]) -> usize {
    let mut index = from;
    while index < bytes.len() {
        match bytes[index] {
            // An escape covers the backslash and one ASCII escape character, so
            // masking both keeps `\"` from closing the literal.
            b'\\' => {
                masked[index] = b' ';
                index += 1;
                if index < bytes.len() {
                    masked[index] = b' ';
                    index += 1;
                }
            }
            b'"' => return index + 1,
            _ => {
                masked[index] = b' ';
                index += 1;
            }
        }
    }
    index
}

/// The content start and hash count of a raw literal beginning at `index`.
///
/// Handles `r"…"`, `r#"…"#` with any number of hashes, and the `br` byte-string
/// forms. A lone `r` that is not followed by hashes and a quote is an
/// identifier, not a literal.
fn raw_literal_start(bytes: &[u8], index: usize) -> Option<(usize, usize)> {
    let mut cursor = index;
    if bytes.get(cursor) == Some(&b'b') {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'r') {
        return None;
    }
    cursor += 1;
    let mut hashes = 0;
    while bytes.get(cursor) == Some(&b'#') {
        hashes += 1;
        cursor += 1;
    }
    (bytes.get(cursor) == Some(&b'"')).then_some((cursor + 1, hashes))
}

/// Masks a raw literal whose content starts at `from`, returning the index after
/// its closing quote and hashes.
fn mask_raw(bytes: &[u8], from: usize, hashes: usize, masked: &mut [u8]) -> usize {
    let mut index = from;
    while index < bytes.len() {
        if bytes[index] == b'"'
            && (1..=hashes).all(|offset| bytes.get(index + offset) == Some(&b'#'))
        {
            return index + hashes + 1;
        }
        masked[index] = b' ';
        index += 1;
    }
    index
}

/// The content range of a char literal starting at `index`, delimiters excluded.
///
/// The escape form scans for the closing quote instead of decoding the escape,
/// because `'\x41'` and `'\u{1F600}'` are longer than `'\n'` and the point is to
/// find where the literal ends, not what it holds.
fn char_literal_contents(source: &str, index: usize) -> Option<(usize, usize)> {
    if source.as_bytes().get(index) != Some(&b'\'') {
        return None;
    }
    let start = index + 1;
    let bytes = source.as_bytes();
    if bytes.get(start) == Some(&b'\\') {
        let mut cursor = start + 1;
        // An escape and its payload stay inside the literal, so the closing
        // quote is at most a dozen bytes away; anything further is source code
        // and not a char literal at all.
        while cursor < bytes.len() && cursor <= start + 12 {
            if bytes[cursor] == b'\'' {
                return Some((start, cursor));
            }
            cursor += 1;
        }
        return None;
    }

    let character = source[start..].chars().next()?;
    let end = start + character.len_utf8();
    (bytes.get(end) == Some(&b'\'')).then_some((start, end))
}

#[cfg(test)]
mod tests {
    use super::mask;

    #[test]
    fn an_attribute_inside_a_string_is_masked() {
        let masked = mask("println!(\"use #[allow(dead_code)] here\");\n");
        assert!(!masked.contains("#[allow"));
        assert!(masked.contains("println!("));
    }

    #[test]
    fn an_escaped_quote_does_not_end_the_string() {
        let masked = mask("let text = \"a \\\" #[allow(x)] b\";\n");
        assert!(!masked.contains("#[allow"), "{masked:?}");
    }

    #[test]
    fn a_raw_string_is_masked_regardless_of_its_hashes() {
        assert!(!mask("let q = r#\"#[allow(x)]\"#;\n").contains("#[allow"));
        assert!(!mask("let q = r##\"#[allow(x)]\"##;\n").contains("#[allow"));
        assert!(!mask("let q = br\"#[allow(x)]\";\n").contains("#[allow"));
    }

    #[test]
    fn a_char_literal_is_masked() {
        assert_eq!(
            mask("let quote = '\"'; let a = 1;\n"),
            "let quote = ' '; let a = 1;\n"
        );
        assert_eq!(mask("let newline = '\\n';\n"), "let newline = '  ';\n");
    }

    #[test]
    fn an_escaped_char_literal_is_not_read_as_source() {
        let masked = mask("let quote = '\\''; let rest = \"#[allow(x)]\";\n");
        assert!(!masked.contains("#[allow"), "{masked:?}");
    }

    #[test]
    fn a_lifetime_is_left_alone() {
        let source = "fn find<'a>(value: &'a str) -> &'a str { value }\n";
        assert_eq!(mask(source), source);
    }

    #[test]
    fn masking_preserves_the_length_of_multibyte_text() {
        let source = "let label = \"内存占用\"; #[allow(dead_code)]\n";
        let masked = mask(source);
        assert_eq!(masked.len(), source.len());
        assert!(masked.contains("#[allow(dead_code)]"));
    }

    #[test]
    fn an_unterminated_string_masks_to_the_end() {
        assert_eq!(
            mask("let a = \"unterminated"),
            format!("let a = \"{}", " ".repeat(12))
        );
    }
}
