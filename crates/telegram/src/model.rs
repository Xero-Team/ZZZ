//! Immutable snapshots shared between the engine thread and the UI.

use std::path::PathBuf;

use crate::error::EngineError;

/// Whether the engine has a live connection to Telegram.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConnectionState {
    #[default]
    Offline,
    Connecting,
    Connected,
}

/// The current step of the sign-in flow.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
        matches!(self, Self::SignedIn { .. })
    }
}

/// The kind of media attached to a message.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MediaKind {
    #[default]
    Photo,
    Video,
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DownloadState {
    #[default]
    NotDownloaded,
    Downloading,
    Downloaded,
    Failed,
}

/// A link preview attached to a message.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WebPageSnapshot {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
}

/// A media attachment.
#[derive(Clone, Debug, Default, PartialEq)]
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
    pub webpage: Option<WebPageSnapshot>,
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
                | MediaKind::Voice
                | MediaKind::Audio
                | MediaKind::Document
                | MediaKind::Sticker
        )
    }
}

/// Delivery state of an outgoing message.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SendState {
    #[default]
    Sent,
    Sending,
    Failed,
}

/// A single message in a conversation.
#[derive(Clone, Debug, PartialEq)]
pub struct MessageSnapshot {
    pub id: i32,
    pub chat_id: i64,
    pub outgoing: bool,
    pub sender_name: String,
    pub timestamp_unix: i64,
    pub text: String,
    /// Present only when the message has formatting entities that map cleanly
    /// to CommonMark.
    pub markdown: Option<String>,
    pub media: Option<MediaSnapshot>,
    pub send_state: SendState,
    pub local_id: Option<u64>,
    pub edited: bool,
}

/// A row in the chat list.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatSnapshot {
    pub id: i64,
    pub title: String,
    pub preview: String,
    /// The kind of the last message when its text is empty, so the panel can
    /// localize a media-only preview instead of showing a blank line.
    pub preview_media: Option<MediaKind>,
    pub timestamp_unix: i64,
    pub unread_count: i32,
    pub pinned: bool,
    pub muted: bool,
    pub avatar_initials: String,
}

/// A search result that can be jumped to.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchHit {
    pub chat_id: i64,
    pub chat_title: String,
    /// The matching message. `None` when the hit is the chat title itself
    /// rather than one of its messages.
    pub message: Option<MessageSnapshot>,
}

/// The complete UI-visible state of the engine.
#[derive(Clone, Debug, Default)]
pub struct ViewModel {
    pub configured: bool,
    pub connection: ConnectionState,
    pub auth: AuthState,
    pub chats: Vec<ChatSnapshot>,
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
    pub search_results: Vec<SearchHit>,
    pub unread_total: u32,
}

impl ViewModel {
    pub fn selected_chat_snapshot(&self) -> Option<&ChatSnapshot> {
        let selected = self.selected_chat?;
        self.chats.iter().find(|chat| chat.id == selected)
    }

    pub fn find_message(&self, local_id: u64) -> Option<&MessageSnapshot> {
        self.history
            .iter()
            .find(|message| message.local_id == Some(local_id))
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
    use super::{AuthState, MediaKind, MediaSnapshot, avatar_initials};

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
}
