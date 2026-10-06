mod conn;
mod message_stream;
mod peer;
mod vfs;

pub use conn::Connection;
pub use peer::*;
pub use proto;
pub use proto::{Receipt, TypedEnvelope, error::*};
pub use vfs::*;
mod macros;

#[cfg(feature = "gpui")]
mod proto_client;
#[cfg(feature = "gpui")]
pub use proto_client::*;

pub const PROTOCOL_VERSION: u32 = 68;
