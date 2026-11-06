//! Converts Telegram message entities into a CommonMark whitelist.
//!
//! Entities with no clean CommonMark mapping (spoilers, custom emoji, unknown
//! entities) are skipped so the rest of the message keeps its formatting. A
//! rendered entity whose offset cannot be mapped still makes the whole
//! conversion fail, so the caller can fall back to plain text.

use grammers_tl_types as tl;
use std::collections::HashSet;

struct Marker {
    start_byte: usize,
    end_byte: usize,
    open: String,
    close: String,
}

/// The representable formatting carried by an entity.
enum EntityKind {
    /// Literal opening and closing markers.
    Markers(&'static str, &'static str),
    /// Inline code, with the fence chosen to survive backticks in the body.
    Code,
    /// A fenced code block with an optional language.
    Pre(Option<String>),
    /// A link whose label is wrapped in `[` and `](url)`.
    Link(String),
}

/// Converts a message body plus its Telegram entities into CommonMark.
///
/// Returns `None` when a rendered entity's offset is out of range or when two
/// formatting spans cross, because either would render the message wrong.
/// Entities with no CommonMark representation are dropped individually. A code
/// block entity that ends one unit past the text is clamped, because clients
/// strip a trailing code block's newline after measuring the entity. Mentions,
/// URLs, emails, phone numbers, hashtags, bot commands, cashtags, underlines,
/// and spoilers are treated as plain text because CommonMark has no equivalent.
/// Nested spans are kept. Literal ASCII punctuation outside code spans is
/// backslash-escaped, so the returned markdown parses back to the same text.
pub fn entities_to_markdown(text: &str, entities: &[tl::enums::MessageEntity]) -> Option<String> {
    if entities.is_empty() {
        return None;
    }

    let mut ranges = Vec::new();
    let mut markers = Vec::new();
    let mut verbatim_ranges = Vec::new();
    for entity in entities {
        let (offset, length, kind) = match entity {
            tl::enums::MessageEntity::Bold(entity) => (
                entity.offset,
                entity.length,
                EntityKind::Markers("**", "**"),
            ),
            tl::enums::MessageEntity::Italic(entity) => {
                (entity.offset, entity.length, EntityKind::Markers("_", "_"))
            }
            tl::enums::MessageEntity::Strike(entity) => (
                entity.offset,
                entity.length,
                EntityKind::Markers("~~", "~~"),
            ),
            tl::enums::MessageEntity::Code(entity) => {
                (entity.offset, entity.length, EntityKind::Code)
            }
            tl::enums::MessageEntity::Pre(entity) => {
                let language = entity.language.trim();
                let language = (!language.is_empty()).then(|| language.to_owned());
                (entity.offset, entity.length, EntityKind::Pre(language))
            }
            tl::enums::MessageEntity::TextUrl(entity) => (
                entity.offset,
                entity.length,
                EntityKind::Link(entity.url.clone()),
            ),
            // CommonMark has no underline. Render it as plain text rather than
            // emitting raw HTML, which the markdown renderer shows literally.
            tl::enums::MessageEntity::Underline(_) => continue,
            tl::enums::MessageEntity::Url(_)
            | tl::enums::MessageEntity::Email(_)
            | tl::enums::MessageEntity::Phone(_)
            | tl::enums::MessageEntity::Mention(_)
            | tl::enums::MessageEntity::MentionName(_)
            | tl::enums::MessageEntity::Hashtag(_)
            | tl::enums::MessageEntity::BotCommand(_)
            | tl::enums::MessageEntity::Cashtag(_) => continue,
            // Spoilers, custom emoji, and entities added by newer layers have
            // no CommonMark equivalent. Skip them rather than discarding the
            // formatting the message does carry.
            _ => continue,
        };

        let start_byte = utf16_offset_to_byte(text, offset)?;
        let end_utf16 = offset.checked_add(length)?;
        let end_byte = match utf16_offset_to_byte(text, end_utf16) {
            Some(end_byte) => end_byte,
            // A code block's entity can end one unit past the text because the
            // sending client stripped the block's trailing newline after
            // measuring the entity. Clamp it so the code block survives instead
            // of dropping every format in the message.
            None if matches!(kind, EntityKind::Pre(_))
                && end_utf16 > text.encode_utf16().count() as i32 =>
            {
                text.len()
            }
            None => return None,
        };
        if start_byte > end_byte {
            return None;
        }
        ranges.push((start_byte, end_byte));

        let content = text.get(start_byte..end_byte)?;
        let (open, close) = match kind {
            EntityKind::Markers(open, close) => (open.to_owned(), close.to_owned()),
            EntityKind::Link(url) => (
                "[".to_owned(),
                format!("]({})", escape_link_destination(&url)),
            ),
            EntityKind::Code => {
                verbatim_ranges.push((start_byte, end_byte));
                code_markers(content)
            }
            EntityKind::Pre(language) => {
                verbatim_ranges.push((start_byte, end_byte));
                let (open, close) = pre_markers(content, language.as_deref());
                // A code block is a block, so its closing fence must be followed
                // by a newline. Otherwise text after the block is glued to the
                // fence (`\`\`\`text`) and no longer parses as a fence.
                (open, format!("{close}\n"))
            }
        };
        markers.push(Marker {
            start_byte,
            end_byte,
            open,
            close,
        });
    }

    if spans_cross(&ranges) {
        return None;
    }

    Some(render_markers(text, markers, &verbatim_ranges))
}

/// Byte positions that need a backslash in the markdown source: every ASCII
/// punctuation character outside a code span. This keeps literal text literal,
/// so `*`, `_`, `[`, a leading `#`, or an escaped backslash survive a parse of
/// the returned markdown. Positions inside a code span are skipped because
/// backslashes are literal there.
fn markdown_escapes(text: &str, verbatim: &[(usize, usize)]) -> HashSet<usize> {
    let mut escapes = HashSet::new();
    for (position, character) in text.char_indices() {
        if character.is_ascii_punctuation()
            && !verbatim
                .iter()
                .any(|&(start, end)| position >= start && position < end)
        {
            escapes.insert(position);
        }
    }
    escapes
}

/// Escapes literal text so that parsing it yields the same text and no
/// formatting. Used when a message has no representable entities, where
/// returning markdown would change the author's literal text.
pub(crate) fn escape_markdown_text(text: &str) -> String {
    let escapes = markdown_escapes(text, &[]);
    let mut output = String::with_capacity(text.len() + escapes.len());
    push_text_segment(&mut output, text, 0, text.len(), &escapes);
    output
}

/// Formats a URL for a link destination. URLs containing characters that would
/// terminate a bare destination use the pointy-bracket form instead.
fn escape_link_destination(url: &str) -> String {
    let needs_brackets = url.chars().any(|character| {
        character.is_whitespace() || matches!(character, '(' | ')' | '<' | '>' | '\\')
    });
    if !needs_brackets {
        return url.to_owned();
    }

    let mut escaped = String::with_capacity(url.len() + 2);
    escaped.push('<');
    for character in url.chars() {
        match character {
            '<' => escaped.push_str("%3C"),
            '>' => escaped.push_str("%3E"),
            ' ' => escaped.push_str("%20"),
            '\n' => escaped.push_str("%0A"),
            '\r' => escaped.push_str("%0D"),
            // A backslash is an escape character even inside a pointy-bracket
            // destination: a trailing one would turn the closing `>` into `\>`,
            // so the link stops parsing. Double it to keep the original URL;
            // percent-encoding here would silently change the URL's value.
            '\\' => escaped.push_str("\\\\"),
            other => escaped.push(other),
        }
    }
    escaped.push('>');
    escaped
}

/// Collects the entities that have a CommonMark representation inside a UTF-16
/// range, clipping spans that cross the range boundary. Entities without a
/// CommonMark form are dropped; callers pass entities produced by the markdown
/// parser, where dropping one is equivalent to the existing plain-text fallback.
pub(crate) fn entities_in_range(
    entities: &[tl::enums::MessageEntity],
    offset: i32,
    length: i32,
) -> Vec<tl::enums::MessageEntity> {
    let Some(range_end) = offset.checked_add(length) else {
        return Vec::new();
    };
    entities
        .iter()
        .filter_map(|entity| clip_entity(entity, offset, range_end))
        .collect()
}

fn clip_entity(
    entity: &tl::enums::MessageEntity,
    range_start: i32,
    range_end: i32,
) -> Option<tl::enums::MessageEntity> {
    fn clip(start: i32, length: i32, range_start: i32, range_end: i32) -> Option<(i32, i32)> {
        let end = start.checked_add(length)?;
        let clipped_start = start.max(range_start);
        let clipped_end = end.min(range_end);
        (clipped_start < clipped_end)
            .then_some((clipped_start - range_start, clipped_end - clipped_start))
    }

    match entity {
        tl::enums::MessageEntity::Bold(entity) => {
            clip(entity.offset, entity.length, range_start, range_end)
                .map(|(offset, length)| tl::types::MessageEntityBold { offset, length }.into())
        }
        tl::enums::MessageEntity::Italic(entity) => {
            clip(entity.offset, entity.length, range_start, range_end)
                .map(|(offset, length)| tl::types::MessageEntityItalic { offset, length }.into())
        }
        tl::enums::MessageEntity::Strike(entity) => {
            clip(entity.offset, entity.length, range_start, range_end)
                .map(|(offset, length)| tl::types::MessageEntityStrike { offset, length }.into())
        }
        tl::enums::MessageEntity::Code(entity) => {
            clip(entity.offset, entity.length, range_start, range_end)
                .map(|(offset, length)| tl::types::MessageEntityCode { offset, length }.into())
        }
        tl::enums::MessageEntity::Pre(entity) => {
            clip(entity.offset, entity.length, range_start, range_end).map(|(offset, length)| {
                tl::types::MessageEntityPre {
                    offset,
                    length,
                    language: entity.language.clone(),
                }
                .into()
            })
        }
        tl::enums::MessageEntity::TextUrl(entity) => {
            clip(entity.offset, entity.length, range_start, range_end).map(|(offset, length)| {
                tl::types::MessageEntityTextUrl {
                    offset,
                    length,
                    url: entity.url.clone(),
                }
                .into()
            })
        }
        _ => None,
    }
}

/// Chooses an inline-code fence that survives any backtick run in `content`.
/// CommonMark pads the code span with a space when the content starts or ends
/// with a backtick, and strips it again when rendering.
fn code_markers(content: &str) -> (String, String) {
    let fence = "`".repeat(longest_backtick_run(content) + 1);
    if content.starts_with('`') || content.ends_with('`') {
        (format!("{fence} "), format!(" {fence}"))
    } else {
        (fence.clone(), fence)
    }
}

/// Chooses a fenced-code-block fence of at least three backticks.
pub(crate) fn pre_markers(content: &str, language: Option<&str>) -> (String, String) {
    let fence = "`".repeat((longest_backtick_run(content) + 1).max(3));
    let open = match language {
        Some(language) => format!("{fence}{language}\n"),
        None => format!("{fence}\n"),
    };
    (open, format!("\n{fence}"))
}

fn longest_backtick_run(content: &str) -> usize {
    content
        .split(|character| character != '`')
        .map(str::len)
        .max()
        .unwrap_or(0)
}

/// Returns true when any two spans partially overlap, which has no CommonMark
/// representation. Nested (fully contained) spans are allowed.
fn spans_cross(ranges: &[(usize, usize)]) -> bool {
    for (index, &(start, end)) in ranges.iter().enumerate() {
        for &(other_start, other_end) in &ranges[index + 1..] {
            let crosses = (start < other_start && other_start < end && end < other_end)
                || (other_start < start && start < other_end && other_end < end);
            if crosses {
                return true;
            }
        }
    }
    false
}

/// Converts a Telegram UTF-16 offset into a byte offset.
fn utf16_offset_to_byte(text: &str, utf16_offset: i32) -> Option<usize> {
    if utf16_offset < 0 {
        return None;
    }
    let target = utf16_offset as usize;
    let mut units = 0;
    for (byte_index, character) in text.char_indices() {
        if units == target {
            return Some(byte_index);
        }
        if units > target {
            return None;
        }
        units += character.len_utf16();
    }
    (units == target).then_some(text.len())
}

/// A delimiter to emit at a byte position, carrying the span it belongs to so
/// markers that share a boundary can be nested correctly.
struct MarkerEvent {
    position: usize,
    is_open: bool,
    span_start: usize,
    span_end: usize,
    text: String,
}

fn render_markers(text: &str, markers: Vec<Marker>, verbatim_ranges: &[(usize, usize)]) -> String {
    let escapes = markdown_escapes(text, verbatim_ranges);
    let mut events = Vec::with_capacity(markers.len() * 2);
    for marker in markers {
        events.push(MarkerEvent {
            position: marker.start_byte,
            is_open: true,
            span_start: marker.start_byte,
            span_end: marker.end_byte,
            text: marker.open,
        });
        events.push(MarkerEvent {
            position: marker.end_byte,
            is_open: false,
            span_start: marker.start_byte,
            span_end: marker.end_byte,
            text: marker.close,
        });
    }
    events.sort_by(|left, right| {
        left.position
            .cmp(&right.position)
            // A closing delimiter at a position is emitted before an opening
            // one, so adjacent spans do not interfere.
            .then_with(|| left.is_open.cmp(&right.is_open))
            .then_with(|| {
                // Nest markers that share a boundary: the outer span (larger
                // end) opens first, and the inner span (later start) closes
                // first. Emitting them by declaration order can produce
                // crossing delimiters that render literally.
                if left.is_open {
                    right.span_end.cmp(&left.span_end)
                } else {
                    right.span_start.cmp(&left.span_start)
                }
            })
            .then_with(|| left.text.cmp(&right.text))
    });

    let mut output = String::with_capacity(text.len() + events.len() * 2);
    let mut cursor = 0;
    let mut event_index = 0;
    while event_index < events.len() {
        let position = events[event_index].position.min(text.len());
        if position > cursor {
            push_text_segment(&mut output, text, cursor, position, &escapes);
            cursor = position;
        }
        while event_index < events.len() && events[event_index].position == position {
            output.push_str(&events[event_index].text);
            event_index += 1;
        }
    }
    if cursor < text.len() {
        push_text_segment(&mut output, text, cursor, text.len(), &escapes);
    }
    output
}

/// Copies `text[from..to]`, inserting a backslash before every byte position in
/// `escapes`.
fn push_text_segment(
    output: &mut String,
    text: &str,
    from: usize,
    to: usize,
    escapes: &HashSet<usize>,
) {
    let mut cursor = from;
    for (offset, _) in text[from..to].char_indices() {
        let position = from + offset;
        if escapes.contains(&position) {
            output.push_str(&text[cursor..position]);
            output.push('\\');
            cursor = position;
        }
    }
    output.push_str(&text[cursor..to]);
}

#[cfg(test)]
mod tests {
    use super::entities_to_markdown;
    use grammers_tl_types as tl;

    fn bold(offset: i32, length: i32) -> tl::enums::MessageEntity {
        tl::types::MessageEntityBold { offset, length }.into()
    }

    fn italic(offset: i32, length: i32) -> tl::enums::MessageEntity {
        tl::types::MessageEntityItalic { offset, length }.into()
    }

    fn code(offset: i32, length: i32) -> tl::enums::MessageEntity {
        tl::types::MessageEntityCode { offset, length }.into()
    }

    fn text_url(offset: i32, length: i32, url: &str) -> tl::enums::MessageEntity {
        tl::types::MessageEntityTextUrl {
            offset,
            length,
            url: url.to_owned(),
        }
        .into()
    }

    fn spoiler(offset: i32, length: i32) -> tl::enums::MessageEntity {
        tl::types::MessageEntitySpoiler { offset, length }.into()
    }

    fn mention(offset: i32, length: i32) -> tl::enums::MessageEntity {
        tl::types::MessageEntityMention { offset, length }.into()
    }

    fn underline(offset: i32, length: i32) -> tl::enums::MessageEntity {
        tl::types::MessageEntityUnderline { offset, length }.into()
    }

    fn pre(offset: i32, length: i32, language: &str) -> tl::enums::MessageEntity {
        tl::types::MessageEntityPre {
            offset,
            length,
            language: language.to_owned(),
        }
        .into()
    }

    #[test]
    fn converts_bold_and_italic() {
        let result = entities_to_markdown("hello world", &[bold(0, 5), italic(6, 5)]).unwrap();
        assert_eq!(result, "**hello** _world_");
    }

    #[test]
    fn converts_inline_code_and_link() {
        let result =
            entities_to_markdown("see docs", &[code(0, 3), text_url(4, 4, "https://x")]).unwrap();
        assert_eq!(result, "`see` [docs](https://x)");
    }

    #[test]
    fn nested_formatting_is_preserved() {
        let result = entities_to_markdown("bold", &[bold(0, 4), italic(0, 4)]).unwrap();
        assert!(result.contains("**"));
        assert!(result.contains('_'));
        assert!(result.contains("bold"));
    }

    #[test]
    fn unmappable_entity_is_skipped() {
        let result = entities_to_markdown("bold secret", &[bold(0, 4), spoiler(5, 6)]).unwrap();
        assert_eq!(result, "**bold** secret");
    }

    #[test]
    fn mention_is_plain_text() {
        let result = entities_to_markdown("@someone", &[mention(0, 8)]);
        assert_eq!(result, Some("\\@someone".to_owned()));
    }

    #[test]
    fn out_of_range_offset_falls_back() {
        assert!(entities_to_markdown("hi", &[bold(0, 99)]).is_none());
    }

    #[test]
    fn unicode_offsets_use_utf16() {
        // "a😀b": the emoji occupies two UTF-16 units, so italic covers "b".
        let result = entities_to_markdown("a😀b", &[italic(3, 1)]).unwrap();
        assert_eq!(result, "a😀_b_");
    }

    #[test]
    fn empty_entities_do_not_convert() {
        assert!(entities_to_markdown("plain", &[]).is_none());
    }

    #[test]
    fn underline_is_plain_text() {
        let result = entities_to_markdown("under", &[underline(0, 5)]);
        assert_eq!(result, Some("under".to_owned()));
    }

    #[test]
    fn inline_code_with_backticks_uses_a_longer_fence() {
        let result = entities_to_markdown("a`b", &[code(0, 3)]).expect("code span");
        assert_eq!(result, "``a`b``");
    }

    #[test]
    fn inline_code_starting_with_backtick_is_padded() {
        let result = entities_to_markdown("`x", &[code(0, 2)]).expect("code span");
        assert_eq!(result, "`` `x ``");
    }

    #[test]
    fn pre_uses_a_fence_and_language() {
        let result = entities_to_markdown("fn main() {}", &[pre(0, 12, "rust")]).expect("pre");
        assert_eq!(result, "```rust\nfn main() {}\n```\n");
    }

    #[test]
    fn crossing_spans_fall_back() {
        // Bold 0..5 and italic 3..8 partially overlap.
        assert!(entities_to_markdown("abcdefgh", &[bold(0, 5), italic(3, 5)]).is_none());
    }

    #[test]
    fn nested_spans_sharing_an_end_close_inner_first() {
        // Bold wraps italic, and both end at the same offset. The inner italic
        // must close first so the delimiters nest instead of crossing.
        let result = entities_to_markdown("abcdef", &[bold(0, 6), italic(2, 4)]).unwrap();
        assert_eq!(result, "**ab_cdef_**");
    }

    #[test]
    fn link_then_code_block_round_trips() {
        let text = "schema.py:21-30\n\nasync def apply() -> None:\n    pass";
        let entities = vec![
            text_url(0, 15, "https://example.com/schema.py#L21-L30"),
            pre(17, 35, "python"),
        ];
        let result = entities_to_markdown(text, &entities);
        assert_eq!(
            result.as_deref(),
            Some(
                "[schema\\.py\\:21\\-30](https://example.com/schema.py#L21-L30)\n\n```python\nasync def apply() -> None:\n    pass\n```\n"
            )
        );
    }

    #[test]
    fn punctuation_inside_formatting_round_trips() {
        let result = entities_to_markdown("a*b", &[bold(0, 3)]).expect("bold");
        assert_eq!(result, "**a\\*b**");

        let (text, entities) = grammers_client::parsers::parse_markdown_message(&result);
        assert_eq!(text, "a*b");
        assert!(entities.iter().any(|entity| matches!(
            entity,
            tl::enums::MessageEntity::Bold(bold) if bold.offset == 0 && bold.length == 3
        )));
    }

    #[test]
    fn literal_punctuation_round_trips_without_entities() {
        let text = "a*b_c[d]e\\f ~g~ # h";
        let escaped = super::escape_markdown_text(text);
        let (parsed, entities) = grammers_client::parsers::parse_markdown_message(&escaped);
        assert_eq!(parsed, text);
        assert!(entities.is_empty(), "literal text gained formatting");
    }

    #[test]
    fn link_label_brackets_are_escaped() {
        let result =
            entities_to_markdown("a]b[c\\d", &[text_url(0, 7, "https://x")]).expect("link");
        assert_eq!(result, r"[a\]b\[c\\d](https://x)");

        let (text, entities) = grammers_client::parsers::parse_markdown_message(&result);
        assert_eq!(text, "a]b[c\\d");
        assert!(entities.iter().any(|entity| matches!(
            entity,
            tl::enums::MessageEntity::TextUrl(url) if url.url == "https://x"
        )));
    }

    #[test]
    fn link_label_code_is_not_escaped() {
        let result = entities_to_markdown("a[b", &[text_url(0, 3, "https://x"), code(1, 2)])
            .expect("link with code");
        assert_eq!(result, "[a`[b`](https://x)");

        let (text, entities) = grammers_client::parsers::parse_markdown_message(&result);
        assert_eq!(text, "a[b");
        assert!(entities.iter().any(|entity| matches!(
            entity,
            tl::enums::MessageEntity::Code(code) if code.offset == 1 && code.length == 2
        )));
    }

    #[test]
    fn link_destination_with_parentheses_uses_brackets() {
        let result =
            entities_to_markdown("docs", &[text_url(0, 4, "https://x/(a)")]).expect("link");
        assert_eq!(result, "[docs](<https://x/(a)>)");

        let (text, entities) = grammers_client::parsers::parse_markdown_message(&result);
        assert_eq!(text, "docs");
        assert!(entities.iter().any(|entity| matches!(
            entity,
            tl::enums::MessageEntity::TextUrl(url) if url.url == "https://x/(a)"
        )));
    }

    #[test]
    fn code_block_entity_one_past_the_text_is_clamped() {
        let text = "schema.py:21-30\n\nasync def apply() -> None:\n    pass";
        // `grammers` reports the code block one unit past the text because it
        // strips the block's trailing newline after measuring the entity.
        let entities = vec![
            text_url(0, 15, "https://example.com/schema.py#L21-L30"),
            pre(17, 36, "python"),
        ];
        let result = entities_to_markdown(text, &entities).expect("clamped code block");
        assert!(result.contains("```python"), "{result}");
        assert!(
            result.contains("async def apply() -> None:\n    pass"),
            "{result}"
        );
    }

    #[test]
    fn nested_spans_sharing_a_start_open_outer_first() {
        // Bold and italic start together; bold ends later so it must open first.
        let result = entities_to_markdown("abcdef", &[italic(0, 2), bold(0, 6)]).unwrap();
        assert_eq!(result, "**_ab_cdef**");
    }

    #[test]
    fn link_destination_with_backslash_round_trips() {
        // A backslash is one of the characters that selects the pointy-bracket
        // destination form, but it stays an active escape character inside that
        // form. A trailing one turns the closing `>` into `\>`, so the whole
        // link stops parsing and the message keeps the raw markdown instead of
        // the link. Escape the backslash so the original URL survives the
        // round-trip unchanged.
        for url in ["https://x/a\\b", "https://x/a\\"] {
            let result = entities_to_markdown("docs", &[text_url(0, 4, url)]).expect("link");
            let (text, entities) = grammers_client::parsers::parse_markdown_message(&result);
            assert_eq!(text, "docs", "link syntax broke for {url:?}: {result:?}");
            assert!(
                entities.iter().any(|entity| matches!(
                    entity,
                    tl::enums::MessageEntity::TextUrl(parsed) if parsed.url == url
                )),
                "URL changed for {url:?}: {result:?}"
            );
        }
    }
}
