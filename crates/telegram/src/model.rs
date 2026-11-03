//! Immutable snapshots shared between the engine thread and the UI.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::EngineError;

/// Whether the engine has a live connection to Telegram.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    #[default]
    Offline,
    Connecting,
    Connected,
}

/// The current step of the sign-in flow.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthState {
    #[default]
    SignedOut,
    AwaitingCode {
        phone: String,
    },
    AwaitingQr {
        url: String,
    },
    AwaitingPassword {
        hint: Option<String>,
    },
    SignedIn,
}

impl AuthState {
    pub fn is_signed_in(&self) -> bool {
        matches!(self, Self::SignedIn)
    }
}

/// What kind of conversation a chat is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatKind {
    #[default]
    User,
    Group,
    Channel,
}

/// A coarse presence state for a user chat.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Presence {
    #[default]
    Unknown,
    Online,
    Recently,
    LastSeen(i64),
}

/// Someone is typing in a chat, until `until_unix`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypingSnapshot {
    pub user_name: String,
    pub until_unix: i64,
}

/// The kind of media attached to a message.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKind {
    #[default]
    Photo,
    Video,
    Animation,
    VideoNote,
    Voice,
    Audio,
    Document,
    Sticker,
    WebPage,
    Contact,
    Poll,
    Geo,
    Other,
}

/// Progress of an on-demand media download.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadState {
    #[default]
    NotDownloaded,
    Downloading,
    Downloaded,
    Failed,
}

/// A link preview attached to a message.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebPageSnapshot {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
}

/// A poll shown in a message.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollSnapshot {
    pub question: String,
    pub options: Vec<PollOptionSnapshot>,
    pub closed: bool,
    pub multiple_choice: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollOptionSnapshot {
    pub text: String,
    pub voters: i32,
}

/// A shared contact card.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactSnapshot {
    pub first_name: String,
    pub last_name: String,
    pub phone: String,
}

/// A shared location or venue.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GeoSnapshot {
    pub latitude: f64,
    pub longitude: f64,
    pub title: Option<String>,
    pub address: Option<String>,
    pub live: bool,
}

/// A media attachment.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaSnapshot {
    pub kind: MediaKind,
    pub file_name: Option<String>,
    pub mime_type: Option<String>,
    pub size: Option<u64>,
    pub duration_seconds: Option<f64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub downloaded_path: Option<PathBuf>,
    pub download_state: DownloadState,
    /// A small preview fetched on demand, cheaper than the original.
    pub thumbnail_path: Option<PathBuf>,
    pub thumbnail_state: DownloadState,
    /// Telegram's pre-rendered voice waveform, packed as two 4-bit levels per
    /// byte. Present only for voice messages that carry it.
    pub waveform: Option<Vec<u8>>,
    pub webpage: Option<WebPageSnapshot>,
    pub poll: Option<PollSnapshot>,
    pub contact: Option<ContactSnapshot>,
    pub geo: Option<GeoSnapshot>,
}

impl MediaSnapshot {
    /// Whether the attachment has downloadable file content. Contacts, polls,
    /// geolocations, link previews, and unknown kinds do not, so the panel must
    /// not offer a Download affordance for them.
    pub fn is_downloadable(&self) -> bool {
        matches!(
            self.kind,
            MediaKind::Photo
                | MediaKind::Video
                | MediaKind::Animation
                | MediaKind::VideoNote
                | MediaKind::Voice
                | MediaKind::Audio
                | MediaKind::Document
                | MediaKind::Sticker
        )
    }

    /// Whether the media can be shown as an image without downloading the
    /// original file.
    pub fn is_previewable(&self) -> bool {
        matches!(
            self.kind,
            MediaKind::Photo | MediaKind::Sticker | MediaKind::Video | MediaKind::Animation
        )
    }

    /// Clears cached paths whose files no longer exist. The operating system may
    /// delete the cache at any time, so a stale path must not become a broken
    /// preview or an Open button for a missing file.
    pub fn forget_missing_cache_files(&mut self) -> bool {
        let mut changed = false;
        if self
            .downloaded_path
            .as_deref()
            .is_some_and(|path| !path.is_file())
        {
            self.downloaded_path = None;
            self.download_state = DownloadState::NotDownloaded;
            changed = true;
        }
        if self
            .thumbnail_path
            .as_deref()
            .is_some_and(|path| !path.is_file())
        {
            self.thumbnail_path = None;
            self.thumbnail_state = DownloadState::NotDownloaded;
            changed = true;
        }
        changed
    }
}

/// Delivery state of an outgoing message.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SendState {
    #[default]
    Sent,
    Sending,
    Failed,
}

/// A compact preview of the message a reply points at.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplySnapshot {
    pub message_id: i32,
    pub sender_name: String,
    pub text: String,
    pub media: Option<MediaKind>,
}

/// A reaction summary on a message.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReactionSnapshot {
    pub emoji: String,
    pub count: i32,
    pub chosen: bool,
}

/// A service message such as a member joining or a title change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageAction {
    MemberJoined {
        actor: String,
    },
    MemberAdded {
        actor: String,
        member: String,
    },
    MemberLeft {
        actor: String,
    },
    TitleChanged {
        actor: String,
        title: String,
    },
    PhotoChanged {
        actor: String,
    },
    PinnedMessage {
        actor: String,
    },
    GroupCreated {
        actor: String,
        title: String,
    },
    ChannelCreated {
        title: String,
    },
    Call {
        outgoing: bool,
        missed: bool,
        duration_seconds: Option<i32>,
    },
    Other,
}

/// A single message in a conversation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessageSnapshot {
    pub id: i32,
    pub chat_id: i64,
    pub outgoing: bool,
    pub sender_name: String,
    pub sender_id: Option<i64>,
    pub timestamp_unix: i64,
    pub text: String,
    /// Present only when the message has formatting entities that map cleanly
    /// to CommonMark.
    pub markdown: Option<String>,
    pub media: Option<MediaSnapshot>,
    pub reply_to: Option<Box<ReplySnapshot>>,
    pub forwarded_from: Option<String>,
    pub grouped_id: Option<i64>,
    pub views: Option<i32>,
    pub action: Option<MessageAction>,
    pub reactions: Vec<ReactionSnapshot>,
    pub send_state: SendState,
    pub local_id: Option<u64>,
    pub edited: bool,
}

impl MessageSnapshot {
    /// Whether the message is a service message that carries no user content.
    pub fn is_service(&self) -> bool {
        self.action.is_some()
    }

    /// A short plain-text preview used for replies, pinned bars, and search.
    pub fn preview_text(&self) -> String {
        if !self.text.is_empty() {
            return self.text.replace('\n', " ");
        }
        if let Some(media) = &self.media {
            if let Some(webpage) = &media.webpage {
                return webpage.title.clone().unwrap_or_else(|| webpage.url.clone());
            }
            if let Some(poll) = &media.poll {
                return poll.question.clone();
            }
            if let Some(contact) = &media.contact {
                return format!("{} {}", contact.first_name, contact.last_name)
                    .trim()
                    .to_owned();
            }
            if let Some(geo) = &media.geo {
                return geo
                    .title
                    .clone()
                    .or_else(|| geo.address.clone())
                    .unwrap_or_default();
            }
        }
        String::new()
    }
}

/// A row in the chat list.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChatSnapshot {
    pub id: i64,
    pub title: String,
    pub username: Option<String>,
    pub kind: ChatKind,
    pub preview: String,
    /// The kind of the last message when its text is empty, so the panel can
    /// localize a media-only preview instead of showing a blank line.
    pub preview_media: Option<MediaKind>,
    pub timestamp_unix: i64,
    pub unread_count: i32,
    pub unread_mentions: i32,
    pub pinned: bool,
    pub muted: bool,
    pub archived: bool,
    /// Whether the current account may send messages to this chat.
    pub can_send: bool,
    pub slow_mode_seconds: u32,
    /// The newest incoming message the account has read.
    pub read_inbox_message_id: i32,
    /// The newest outgoing message the other side has read.
    pub read_outbox_message_id: i32,
    pub avatar_initials: String,
    pub draft: Option<String>,
    pub typing: Option<TypingSnapshot>,
    pub pinned_message: Option<Box<MessageSnapshot>>,
    pub presence: Presence,
}

/// A search result that can be jumped to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub chat_id: i64,
    pub chat_title: String,
    /// The matching message. `None` when the hit is the chat title itself
    /// rather than one of its messages.
    pub message: Option<MessageSnapshot>,
}

/// A chat folder exposed by Telegram.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatFolderSnapshot {
    pub id: i32,
    pub title: String,
}

/// Which messages a search should return.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchFilter {
    #[default]
    All,
    Text,
    Media,
    Links,
    Files,
}

/// An explicit chat folder selection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FolderSelection {
    #[default]
    All,
    Archive,
    Folder(i32),
}

/// The complete UI-visible state of the engine.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ViewModel {
    pub configured: bool,
    pub connection: ConnectionState,
    pub auth: AuthState,
    pub chats: Vec<ChatSnapshot>,
    pub folders: Vec<ChatFolderSnapshot>,
    pub selected_folder: FolderSelection,
    pub selected_chat: Option<i64>,
    pub history: Vec<MessageSnapshot>,
    pub draft: String,
    /// A structured error. The engine never builds user-facing strings; the
    /// panel localizes this value.
    pub error: Option<EngineError>,
    /// Seconds from a `FLOOD_WAIT` error, for the panel's countdown.
    pub flood_wait_seconds: Option<u64>,
    pub loading_chats: bool,
    pub loading_history: bool,
    pub has_more_history: bool,
    pub searching: bool,
    pub search_query: String,
    pub search_filter: SearchFilter,
    pub search_scope: Option<i64>,
    pub has_more_search: bool,
    pub search_results: Vec<SearchHit>,
    pub unread_total: u32,
}

impl ViewModel {
    pub fn selected_chat_snapshot(&self) -> Option<&ChatSnapshot> {
        let selected = self.selected_chat?;
        self.chats.iter().find(|chat| chat.id == selected)
    }
}

/// Derives up-to-two-character initials from a chat title for the avatar
/// fallback when no profile photo is available.
pub fn avatar_initials(title: &str) -> String {
    let mut initials = String::new();
    for word in title.split_whitespace() {
        if let Some(character) = word.chars().next() {
            initials.extend(character.to_uppercase());
            if initials.chars().count() >= 2 {
                break;
            }
        }
    }
    if initials.is_empty() {
        if let Some(character) = title.chars().next() {
            initials.extend(character.to_uppercase());
        }
    }
    initials
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        AuthState, ChatKind, ChatSnapshot, DownloadState, FolderSelection, MediaKind,
        MediaSnapshot, MessageAction, MessageSnapshot, SearchFilter, SendState, avatar_initials,
    };

    #[test]
    fn initials_from_words() {
        assert_eq!(avatar_initials("Ada Lovelace"), "AL");
        assert_eq!(avatar_initials("telegram"), "T");
    }

    #[test]
    fn initials_handle_empty() {
        assert_eq!(avatar_initials(""), "");
    }

    #[test]
    fn auth_state_reports_signed_in() {
        assert!(!AuthState::SignedOut.is_signed_in());
        assert!(AuthState::SignedIn.is_signed_in());
    }

    #[test]
    fn downloadable_kinds_exclude_metadata_attachments() {
        for kind in [
            MediaKind::Photo,
            MediaKind::Video,
            MediaKind::Animation,
            MediaKind::VideoNote,
            MediaKind::Voice,
            MediaKind::Audio,
            MediaKind::Document,
            MediaKind::Sticker,
        ] {
            assert!(
                MediaSnapshot {
                    kind,
                    ..Default::default()
                }
                .is_downloadable(),
                "{kind:?} should be downloadable"
            );
        }
        for kind in [
            MediaKind::WebPage,
            MediaKind::Contact,
            MediaKind::Poll,
            MediaKind::Geo,
            MediaKind::Other,
        ] {
            assert!(
                !MediaSnapshot {
                    kind,
                    ..Default::default()
                }
                .is_downloadable(),
                "{kind:?} should not be downloadable"
            );
        }
    }

    #[test]
    fn missing_cache_files_are_forgotten() {
        let mut media = MediaSnapshot {
            downloaded_path: Some(PathBuf::from("/nonexistent/zzz/media/1.png")),
            download_state: DownloadState::Downloaded,
            thumbnail_path: Some(PathBuf::from("/nonexistent/zzz/thumbnails/1.jpg")),
            thumbnail_state: DownloadState::Downloaded,
            ..Default::default()
        };

        assert!(media.forget_missing_cache_files());
        assert_eq!(media.downloaded_path, None);
        assert_eq!(media.download_state, DownloadState::NotDownloaded);
        assert_eq!(media.thumbnail_path, None);
        assert_eq!(media.thumbnail_state, DownloadState::NotDownloaded);
        assert!(!media.forget_missing_cache_files());
    }

    #[test]
    fn existing_cache_files_are_kept() {
        let path = std::env::temp_dir().join(format!(
            "zzz-telegram-cached-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        std::fs::write(&path, b"cached").expect("write cache file");

        let mut media = MediaSnapshot {
            downloaded_path: Some(path.clone()),
            download_state: DownloadState::Downloaded,
            ..Default::default()
        };
        assert!(!media.forget_missing_cache_files());
        assert_eq!(media.downloaded_path, Some(path.clone()));

        std::fs::remove_file(&path).expect("remove cache file");
    }

    fn message(id: i32) -> MessageSnapshot {
        MessageSnapshot {
            id,
            chat_id: 1,
            outgoing: false,
            sender_name: "Ada".to_owned(),
            sender_id: Some(1),
            timestamp_unix: 0,
            text: "hello".to_owned(),
            markdown: None,
            media: None,
            reply_to: None,
            forwarded_from: None,
            grouped_id: None,
            views: None,
            action: None,
            reactions: Vec::new(),
            send_state: SendState::Sent,
            local_id: None,
            edited: false,
        }
    }

    #[test]
    fn service_messages_are_detected() {
        let mut service = message(1);
        assert!(!service.is_service());
        service.action = Some(MessageAction::Other);
        assert!(service.is_service());
    }

    #[test]
    fn preview_text_falls_back_to_media() {
        let mut media_message = message(1);
        media_message.text.clear();
        media_message.media = Some(MediaSnapshot {
            kind: MediaKind::Poll,
            poll: Some(super::PollSnapshot {
                question: "Lunch?".to_owned(),
                ..Default::default()
            }),
            ..Default::default()
        });
        assert_eq!(media_message.preview_text(), "Lunch?");
    }

    #[test]
    fn folder_selection_defaults_to_all() {
        assert_eq!(FolderSelection::default(), FolderSelection::All);
        assert_eq!(SearchFilter::default(), SearchFilter::All);
    }

    #[test]
    fn chat_kind_defaults_to_user() {
        assert_eq!(ChatKind::default(), ChatKind::User);
        assert_eq!(ChatSnapshot::default().kind, ChatKind::User);
    }
}
