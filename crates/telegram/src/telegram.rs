//! GUI-independent Telegram engine for ZZZ.
//!
//! The engine owns a grammers client and runs it on a dedicated multi-threaded
//! Tokio runtime on its own OS thread. Callers send [`engine::Command`]s and read
//! immutable [`model::ViewModel`] snapshots, so none of the networking or file
//! work touches the UI thread.

pub mod compose;
pub mod credentials;
pub mod engine;
pub mod error;
pub mod markdown;
pub mod model;
pub mod proxy;
pub mod session;

pub use compose::{
    ALBUM_MAX_ITEMS, AlbumItem, AttachmentKind, CAPTION_LIMIT, ComposerBlock, MESSAGE_TEXT_LIMIT,
    OutgoingItem, StagedAttachment, code_reference_markdown, format_code_fence,
    outgoing_message_count, plan_outgoing,
};
pub use credentials::TelegramCredentials;
pub use engine::{Command, EngineConfig, EngineHandle};
pub use error::EngineError;
pub use model::{
    AuthState, ChatSnapshot, ConnectionState, DownloadState, MediaKind, MediaSnapshot,
    MessageSnapshot, SearchHit, SendState, ViewModel, WebPageSnapshot,
};
