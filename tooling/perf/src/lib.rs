#![warn(missing_docs)]
#![warn(
    clippy::all,
    clippy::pedantic,
    clippy::style,
    clippy::missing_docs_in_private_items
)]
#![deny(
    clippy::as_underscore,
    clippy::allow_attributes,
    clippy::allow_attributes_without_reason
)]
#![forbid(
    clippy::let_underscore_must_use,
    clippy::undocumented_unsafe_blocks,
    clippy::missing_safety_doc
)]

//! Some constants and datatypes used in the Zed perf profiler. Should only be
//! consumed by the crate providing the matching macros.
//!
//! For usage documentation, see the docs on this crate's binary.

mod implementation;
pub use implementation::*;
