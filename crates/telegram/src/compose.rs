//! Outgoing message planning: turns the composer's ordered blocks into the
//! sequence of Telegram messages to send.
//!
//! Telegram messages interleave text and media the same way the composer does
//! not: a text message is either plain text or a media caption. We therefore
//! split the document into runs — consecutive text becomes one (or more) text
//! messages, and each attachment becomes a media message whose caption is the
//! text that immediately follows it.

use std::path::Path;

use grammers_client::media::InputMedia;
use grammers_client::message::InputMessage;
use grammers_client::tl;

/// Telegram's upper bound for a text message, in UTF-16 code units.
pub const MESSAGE_TEXT_LIMIT: usize = 4096;
/// Telegram's upper bound for a media caption, in UTF-16 code units.
pub const CAPTION_LIMIT: usize = 1024;

/// How an attached file is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachmentKind {
    Photo,
    Video,
    Document,
}

impl AttachmentKind {
    /// Classifies a file into a Telegram media kind. Photos are compressed by
    /// Telegram; videos and everything else are sent as documents.
    pub fn classify(path: &Path) -> Self {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase);
        match extension.as_deref() {
            Some(
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tiff" | "tif" | "heic" | "avif",
            ) => Self::Photo,
            Some(
                "mp4" | "mov" | "m4v" | "mkv" | "webm" | "avi" | "wmv" | "flv" | "3gp" | "mpeg"
                | "mpg",
            ) => Self::Video,
            _ => Self::Document,
        }
    }

    /// Whether the kind is rendered as an inline preview rather than a file
    /// row in the composer's attachment chip.
    pub fn is_previewable(self) -> bool {
        matches!(self, Self::Photo | Self::Video)
    }

    /// The `MediaKind` used to render the optimistic message in the UI.
    pub fn media_kind(self) -> crate::model::MediaKind {
        match self {
            Self::Photo => crate::model::MediaKind::Photo,
            Self::Video => crate::model::MediaKind::Video,
            Self::Document => crate::model::MediaKind::Document,
        }
    }
}

/// A file staged in the composer, ready to upload.
#[derive(Clone, Debug, PartialEq)]
pub struct StagedAttachment {
    pub path: std::path::PathBuf,
    pub kind: AttachmentKind,
    pub file_name: String,
    pub size: u64,
}

impl StagedAttachment {
    /// Builds an attachment from an absolute path. Returns `None` for
    /// directories, which Telegram cannot upload.
    pub fn from_path(path: std::path::PathBuf, size: u64) -> Option<Self> {
        let file_name = path.file_name()?.to_string_lossy().into_owned();
        Some(Self {
            kind: AttachmentKind::classify(&path),
            path,
            file_name,
            size,
        })
    }
}

/// A block of the composer's document, in document order.
#[derive(Clone, Debug, PartialEq)]
pub enum ComposerBlock {
    Text(String),
    Attachment(StagedAttachment),
}

/// One member of an album.
#[derive(Clone, Debug, PartialEq)]
pub struct AlbumItem {
    pub attachment: StagedAttachment,
    pub caption: Option<String>,
}

/// One message to send.
#[derive(Clone, Debug, PartialEq)]
pub enum OutgoingItem {
    Text(String),
    Media {
        attachment: StagedAttachment,
        caption: Option<String>,
    },
    /// Two to ten previewable attachments sent as one Telegram album.
    Album {
        items: Vec<AlbumItem>,
    },
}

/// The maximum number of media Telegram allows in one album.
pub const ALBUM_MAX_ITEMS: usize = 10;

impl OutgoingItem {
    /// How many Telegram messages this item produces.
    pub fn message_count(&self) -> usize {
        match self {
            Self::Album { items } => items.len(),
            Self::Text(_) | Self::Media { .. } => 1,
        }
    }
}

/// The total number of Telegram messages a plan produces.
pub fn outgoing_message_count(items: &[OutgoingItem]) -> usize {
    items.iter().map(OutgoingItem::message_count).sum()
}

/// Plans the outgoing messages for a document. Pure and deterministic so it can
/// be unit-tested without a running engine.
pub fn plan_outgoing(blocks: Vec<ComposerBlock>) -> Vec<OutgoingItem> {
    let mut items = Vec::new();
    let mut pending = String::new();
    let mut blocks = blocks.into_iter().peekable();

    while let Some(block) = blocks.next() {
        match block {
            ComposerBlock::Text(text) => pending.push_str(&text),
            ComposerBlock::Attachment(attachment) => {
                push_text(&mut items, &mut pending);
                let caption = match blocks.next_if(|block| matches!(block, ComposerBlock::Text(_)))
                {
                    Some(ComposerBlock::Text(text)) => Some(text),
                    _ => None,
                };
                let (caption, remainder) = match caption {
                    Some(caption) => {
                        let (caption, remainder) = split_caption(caption);
                        (Some(caption), remainder)
                    }
                    None => (None, None),
                };
                items.push(OutgoingItem::Media {
                    attachment,
                    caption,
                });
                if let Some(mut remainder) = remainder {
                    push_text(&mut items, &mut remainder);
                }
            }
        }
    }

    push_text(&mut items, &mut pending);
    merge_albums(items)
}

/// Combines runs of consecutive previewable media into albums of at most
/// [`ALBUM_MAX_ITEMS`].
fn merge_albums(items: Vec<OutgoingItem>) -> Vec<OutgoingItem> {
    let mut merged = Vec::with_capacity(items.len());
    let mut album: Vec<AlbumItem> = Vec::new();

    for item in items {
        if let OutgoingItem::Media {
            attachment,
            caption,
        } = item
        {
            if attachment.kind.is_previewable() {
                album.push(AlbumItem {
                    attachment,
                    caption,
                });
                if album.len() == ALBUM_MAX_ITEMS {
                    flush_album(&mut album, &mut merged);
                }
                continue;
            }
            flush_album(&mut album, &mut merged);
            merged.push(OutgoingItem::Media {
                attachment,
                caption,
            });
        } else {
            flush_album(&mut album, &mut merged);
            merged.push(item);
        }
    }

    flush_album(&mut album, &mut merged);
    merged
}

/// Emits a pending album, degrading a single item back to a plain media
/// message.
///
/// Telegram renders only the first album item's caption, so any captions on
/// later items are hoisted onto the first item. A combined caption longer than
/// [`CAPTION_LIMIT`] overflows into a following text message rather than being
/// dropped.
fn flush_album(album: &mut Vec<AlbumItem>, items: &mut Vec<OutgoingItem>) {
    let combined = album
        .iter_mut()
        .filter_map(|item| item.caption.take())
        .filter(|caption| !caption.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    let mut overflow = None;
    if !combined.is_empty() {
        let (caption, remainder) = split_caption(combined);
        if let Some(first) = album.first_mut() {
            first.caption = (!caption.is_empty()).then_some(caption);
        }
        overflow = remainder;
    }

    match album.len() {
        0 => {}
        1 => {
            let only = album.pop().expect("checked length");
            items.push(OutgoingItem::Media {
                attachment: only.attachment,
                caption: only.caption,
            });
        }
        _ => items.push(OutgoingItem::Album {
            items: std::mem::take(album),
        }),
    }

    if let Some(mut overflow) = overflow {
        push_text(items, &mut overflow);
    }
}

/// Flushes accumulated text as one or more text messages, splitting at
/// [`MESSAGE_TEXT_LIMIT`]. Whitespace-only runs are dropped.
fn push_text(items: &mut Vec<OutgoingItem>, pending: &mut String) {
    let text = pending.trim();
    if !text.is_empty() {
        for part in split_text(text, MESSAGE_TEXT_LIMIT) {
            items.push(OutgoingItem::Text(part));
        }
    }
    pending.clear();
}

/// Splits `text` into chunks of at most `limit` UTF-16 code units, never
/// splitting a scalar value in half.
fn split_text(text: &str, limit: usize) -> Vec<String> {
    if utf16_len(text) <= limit {
        return vec![text.to_owned()];
    }
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut units = 0;
    for character in text.chars() {
        let width = character.len_utf16();
        if units + width > limit && !current.is_empty() {
            parts.push(std::mem::take(&mut current));
            units = 0;
        }
        current.push(character);
        units += width;
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// Truncates a caption to [`CAPTION_LIMIT`], returning the overflow to send as
/// a follow-up text message.
fn split_caption(caption: String) -> (String, Option<String>) {
    if utf16_len(&caption) <= CAPTION_LIMIT {
        return (caption, None);
    }
    let mut kept = String::new();
    let mut overflow = String::new();
    let mut units = 0;
    for character in caption.chars() {
        if overflow.is_empty() && units + character.len_utf16() <= CAPTION_LIMIT {
            kept.push(character);
            units += character.len_utf16();
        } else {
            overflow.push(character);
        }
    }
    let overflow = overflow.trim();
    (kept, (!overflow.is_empty()).then(|| overflow.to_owned()))
}

/// Counts UTF-16 code units, which is how Telegram measures message length.
fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// Builds a CommonMark fenced code block. The fence is longer than any backtick
/// run in `text` so the content cannot terminate it early.
pub fn format_code_fence(language: Option<&str>, text: &str) -> String {
    let (open, close) = crate::markdown::pre_markers(text, language);
    format!("{open}{text}{close}")
}

/// Builds the Markdown for a code reference: a link to the code's target
/// followed by the selected text in a fenced block.
pub fn code_reference_markdown(
    display: &str,
    target: &str,
    language: Option<&str>,
    text: &str,
) -> String {
    format!(
        "[{display}]({target})\n{}",
        format_code_fence(language, text)
    )
}

/// Parses CommonMark into plain text and Telegram entities, clamping any code
/// block entity that extends past the text.
///
/// `grammers` measures a trailing fenced code block's entity against the text
/// before it strips the block's trailing newline, so the entity can end one
/// UTF-16 unit past the text. Telegram drops such an entity, which silently
/// loses the code block, so the length is clamped here.
pub fn parse_markdown_with_clamped_code_blocks(
    source: &str,
) -> (String, Vec<tl::enums::MessageEntity>) {
    let (text, entities) = grammers_client::parsers::parse_markdown_message(source);
    let text_length = text.encode_utf16().count() as i32;
    let entities = entities
        .into_iter()
        .map(|entity| clamp_code_block_entity(entity, text_length))
        .collect();
    (text, entities)
}

/// Builds a text message from CommonMark with entities that stay in bounds.
pub fn markdown_message(source: &str) -> InputMessage {
    let (text, entities) = parse_markdown_with_clamped_code_blocks(source);
    InputMessage::new().text(text).fmt_entities(entities)
}

/// Builds a media caption from CommonMark with entities that stay in bounds.
pub fn markdown_caption(source: &str) -> InputMedia {
    let (text, entities) = parse_markdown_with_clamped_code_blocks(source);
    InputMedia::new().caption(text).fmt_entities(entities)
}

fn clamp_code_block_entity(
    entity: tl::enums::MessageEntity,
    text_length: i32,
) -> tl::enums::MessageEntity {
    match entity {
        tl::enums::MessageEntity::Pre(mut pre)
            if pre.offset.saturating_add(pre.length) > text_length =>
        {
            pre.length = (text_length - pre.offset).max(0);
            tl::enums::MessageEntity::Pre(pre)
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        AlbumItem, AttachmentKind, CAPTION_LIMIT, ComposerBlock, OutgoingItem, StagedAttachment,
        code_reference_markdown, format_code_fence, parse_markdown_with_clamped_code_blocks,
        plan_outgoing, tl,
    };

    fn attachment(name: &str) -> StagedAttachment {
        StagedAttachment {
            path: PathBuf::from(format!("/tmp/{name}")),
            kind: AttachmentKind::Photo,
            file_name: name.to_owned(),
            size: 10,
        }
    }

    #[test]
    fn text_only_blocks_coalesce() {
        let items = plan_outgoing(vec![
            ComposerBlock::Text("hello ".into()),
            ComposerBlock::Text("world".into()),
        ]);
        assert_eq!(items, vec![OutgoingItem::Text("hello world".into())]);
    }

    #[test]
    fn attachment_takes_following_text_as_caption() {
        let file = attachment("a.png");
        let items = plan_outgoing(vec![
            ComposerBlock::Text("look:".into()),
            ComposerBlock::Attachment(file.clone()),
            ComposerBlock::Text("a caption".into()),
        ]);
        assert_eq!(
            items,
            vec![
                OutgoingItem::Text("look:".into()),
                OutgoingItem::Media {
                    attachment: file,
                    caption: Some("a caption".into()),
                },
            ]
        );
    }

    #[test]
    fn attachment_without_following_text_has_no_caption() {
        let file = attachment("a.png");
        let items = plan_outgoing(vec![ComposerBlock::Attachment(file.clone())]);
        assert_eq!(
            items,
            vec![OutgoingItem::Media {
                attachment: file,
                caption: None,
            }]
        );
    }

    #[test]
    fn consecutive_photos_merge_into_an_album() {
        let first = attachment("a.png");
        let second = attachment("b.png");
        let items = plan_outgoing(vec![
            ComposerBlock::Attachment(first.clone()),
            ComposerBlock::Attachment(second.clone()),
        ]);
        assert_eq!(
            items,
            vec![OutgoingItem::Album {
                items: vec![
                    AlbumItem {
                        attachment: first,
                        caption: None,
                    },
                    AlbumItem {
                        attachment: second,
                        caption: None,
                    },
                ],
            }]
        );
    }

    #[test]
    fn album_hoists_later_captions_onto_the_first_item() {
        let first = attachment("a.png");
        let second = attachment("b.png");
        let items = plan_outgoing(vec![
            ComposerBlock::Attachment(first.clone()),
            ComposerBlock::Text("first caption".into()),
            ComposerBlock::Attachment(second.clone()),
            ComposerBlock::Text("second caption".into()),
        ]);
        assert_eq!(
            items,
            vec![OutgoingItem::Album {
                items: vec![
                    AlbumItem {
                        attachment: first,
                        caption: Some("first caption\n\nsecond caption".into()),
                    },
                    AlbumItem {
                        attachment: second,
                        caption: None,
                    },
                ],
            }]
        );
    }

    #[test]
    fn a_document_breaks_an_album() {
        let photo = attachment("a.png");
        let document = StagedAttachment {
            kind: AttachmentKind::Document,
            ..attachment("notes.txt")
        };
        let items = plan_outgoing(vec![
            ComposerBlock::Attachment(photo.clone()),
            ComposerBlock::Attachment(document.clone()),
            ComposerBlock::Attachment(attachment("b.png")),
        ]);
        assert_eq!(
            items,
            vec![
                OutgoingItem::Media {
                    attachment: photo,
                    caption: None,
                },
                OutgoingItem::Media {
                    attachment: document,
                    caption: None,
                },
                OutgoingItem::Media {
                    attachment: attachment("b.png"),
                    caption: None,
                },
            ]
        );
    }

    #[test]
    fn long_text_is_split() {
        let text = "a".repeat(5000);
        let items = plan_outgoing(vec![ComposerBlock::Text(text)]);
        assert_eq!(items.len(), 2);
        let total: usize = items
            .iter()
            .map(|item| match item {
                OutgoingItem::Text(text) => text.chars().count(),
                OutgoingItem::Media { .. } | OutgoingItem::Album { .. } => 0,
            })
            .sum();
        assert_eq!(total, 5000);
    }

    #[test]
    fn splitting_counts_utf16_units() {
        // 3000 emoji are 3000 scalar values but 6000 UTF-16 code units, so they
        // must still split even though they fit within a scalar-value limit.
        let text = "\u{1F600}".repeat(3000);
        let parts = super::split_text(&text, super::MESSAGE_TEXT_LIMIT);
        assert_eq!(parts.len(), 2);
        assert!(
            parts
                .iter()
                .all(|part| super::utf16_len(part) <= super::MESSAGE_TEXT_LIMIT)
        );
        assert_eq!(
            parts.iter().map(|part| part.chars().count()).sum::<usize>(),
            3000
        );
    }

    #[test]
    fn long_caption_overflows_into_a_text_message() {
        let file = attachment("a.png");
        let caption = "b".repeat(CAPTION_LIMIT + 10);
        let items = plan_outgoing(vec![
            ComposerBlock::Attachment(file),
            ComposerBlock::Text(caption),
        ]);
        assert_eq!(items.len(), 2);
        match &items[0] {
            OutgoingItem::Media { caption, .. } => {
                assert_eq!(
                    caption.as_ref().map(|c| c.chars().count()),
                    Some(CAPTION_LIMIT)
                );
            }
            other => panic!("expected media, got {other:?}"),
        }
        match &items[1] {
            OutgoingItem::Text(text) => assert_eq!(text.chars().count(), 10),
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn fence_extends_past_backticks() {
        let fenced = format_code_fence(Some("rust"), "let x = ```y```;");
        assert!(fenced.starts_with("````rust\n"));
        assert!(fenced.ends_with("\n````"));
    }

    #[test]
    fn code_reference_links_then_fences() {
        let markdown =
            code_reference_markdown("a.rs:1-2", "path/a.rs#L1-L2", Some("rust"), "fn a() {}");
        assert_eq!(
            markdown,
            "[a.rs:1-2](path/a.rs#L1-L2)\n```rust\nfn a() {}\n```"
        );
    }

    #[test]
    fn code_reference_followed_by_text_does_not_glue_to_the_fence() {
        let reference = code_reference_markdown("a.rs:1", "a.rs#L1-L1", Some("rust"), "fn a() {}");
        // The expanded reference always ends with a newline so text typed after
        // the chip starts on its own line.
        let markdown = format!("{reference}\n测试");
        let (text, entities) = parse_markdown_with_clamped_code_blocks(&markdown);
        let converted = crate::markdown::entities_to_markdown(&text, &entities)
            .expect("code reference should convert back");
        assert!(
            converted.contains("```rust"),
            "code fence lost: {converted}"
        );
        assert!(
            converted.contains("测试"),
            "trailing text lost: {converted}"
        );
        assert!(
            !converted.contains("```测试"),
            "trailing text glued to the fence: {converted}"
        );
    }

    #[test]
    fn code_reference_with_json_keeps_its_code_block() {
        let content = "// ZZZ keymap\n[\n  {\n    \"context\": \"Workspace\",\n  },\n]";
        let markdown = code_reference_markdown(
            "keymap.json:1-5",
            "https://example.com/keymap.json#L1-L5",
            Some("json"),
            content,
        );
        let (text, entities) = parse_markdown_with_clamped_code_blocks(&markdown);
        let converted = crate::markdown::entities_to_markdown(&text, &entities)
            .expect("code reference should convert back");
        assert!(
            converted.contains("```json"),
            "json code fence lost: {converted}"
        );
        assert!(converted.contains(content), "code body lost: {converted}");
    }

    #[test]
    fn code_reference_parses_as_link_and_code_block() {
        let markdown = code_reference_markdown(
            "schema.py:21-30",
            "https://example.com/schema.py#L21-L30",
            Some("python"),
            "async def apply() -> None:\n    pass",
        );
        let (text, entities) = parse_markdown_with_clamped_code_blocks(&markdown);
        assert_eq!(
            text,
            "schema.py:21-30\n\nasync def apply() -> None:\n    pass"
        );
        assert!(
            entities
                .iter()
                .any(|entity| matches!(entity, tl::enums::MessageEntity::TextUrl(_))),
            "expected a text URL entity, got {entities:?}"
        );
        let text_length = text.encode_utf16().count() as i32;
        let pre = entities
            .iter()
            .find_map(|entity| match entity {
                tl::enums::MessageEntity::Pre(pre) => Some(pre),
                _ => None,
            })
            .expect("expected a code block entity");
        assert_eq!(pre.language, "python");
        assert!(
            pre.offset + pre.length <= text_length,
            "code block entity overruns the text: {pre:?} vs {text_length}"
        );

        let converted = crate::markdown::entities_to_markdown(&text, &entities)
            .expect("entities should convert back");
        assert!(
            converted.contains("```python"),
            "code fence lost on round trip: {converted}"
        );
        assert!(
            converted.contains("[schema.py:21-30](https://example.com/schema.py#L21-L30)"),
            "link lost on round trip: {converted}"
        );
    }

    #[test]
    fn classify_by_extension() {
        assert_eq!(
            AttachmentKind::classify(&PathBuf::from("x.JPG")),
            AttachmentKind::Photo
        );
        assert_eq!(
            AttachmentKind::classify(&PathBuf::from("x.mp4")),
            AttachmentKind::Video
        );
        assert_eq!(
            AttachmentKind::classify(&PathBuf::from("x.rs")),
            AttachmentKind::Document
        );
    }
}
