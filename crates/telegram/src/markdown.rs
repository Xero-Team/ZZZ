//! Converts Telegram message entities into a CommonMark whitelist.
//!
//! Anything that has no clean CommonMark mapping (spoilers, custom emoji,
//! unknown entities) makes the whole conversion fail so the caller can fall
//! back to plain text instead of rendering the message wrong.

use grammers_tl_types as tl;

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
/// Returns `None` when an entity cannot be represented, when any entity offset
/// is out of range, or when two formatting spans cross. Mentions, URLs, emails,
/// phone numbers, hashtags, bot commands, cashtags, and underlines are treated
/// as plain text because CommonMark has no equivalent. Nested spans are kept.
pub fn entities_to_markdown(text: &str, entities: &[tl::enums::MessageEntity]) -> Option<String> {
    if entities.is_empty() {
        return None;
    }

    let mut ranges = Vec::new();
    let mut markers = Vec::new();
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
            _ => return None,
        };

        let start_byte = utf16_offset_to_byte(text, offset)?;
        let end_byte = utf16_offset_to_byte(text, offset.checked_add(length)?)?;
        if start_byte > end_byte {
            return None;
        }
        ranges.push((start_byte, end_byte));

        let content = text.get(start_byte..end_byte)?;
        let (open, close) = match kind {
            EntityKind::Markers(open, close) => (open.to_owned(), close.to_owned()),
            EntityKind::Link(url) => ("[".to_owned(), format!("]({url})")),
            EntityKind::Code => code_markers(content),
            EntityKind::Pre(language) => pre_markers(content, language.as_deref()),
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

    Some(render_markers(text, markers))
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
fn pre_markers(content: &str, language: Option<&str>) -> (String, String) {
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

fn render_markers(text: &str, markers: Vec<Marker>) -> String {
    let mut events: Vec<(usize, u8, usize, String)> = Vec::with_capacity(markers.len() * 2);
    for (index, marker) in markers.into_iter().enumerate() {
        events.push((marker.start_byte, 1, index, marker.open));
        events.push((marker.end_byte, 0, index, marker.close));
    }
    events.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });

    let mut output = String::with_capacity(text.len() + events.len() * 2);
    let mut cursor = 0;
    let mut event_index = 0;
    while event_index < events.len() {
        let position = events[event_index].0.min(text.len());
        if position > cursor {
            output.push_str(&text[cursor..position]);
            cursor = position;
        }
        while event_index < events.len() && events[event_index].0 == position {
            output.push_str(&events[event_index].3);
            event_index += 1;
        }
    }
    if cursor < text.len() {
        output.push_str(&text[cursor..]);
    }
    output
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
    fn unmappable_entity_falls_back() {
        assert!(entities_to_markdown("secret", &[spoiler(0, 6)]).is_none());
    }

    #[test]
    fn mention_is_plain_text() {
        let result = entities_to_markdown("@someone", &[mention(0, 8)]);
        assert_eq!(result, Some("@someone".to_owned()));
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
        assert_eq!(result, "```rust\nfn main() {}\n```");
    }

    #[test]
    fn crossing_spans_fall_back() {
        // Bold 0..5 and italic 3..8 partially overlap.
        assert!(entities_to_markdown("abcdefgh", &[bold(0, 5), italic(3, 5)]).is_none());
    }
}
