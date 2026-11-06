//! Local crash reporting.
//!
//! ZZZ does not upload crashes. When enabled, a sidecar process inspects the
//! editor's memory after a panic and writes a minidump plus a metadata file to
//! the logs directory. Nothing here contacts the network; enabling the sidecar
//! is opt-in through `ZZZ_GENERATE_MINIDUMPS` (see the privacy boundary docs).
//!
//! The backing minidump stack (`crash-handler` / `minidumper` and their
//! `minidump-writer` dependency) only ships Linux, Windows, and macOS backends.
//! Those crates fail to compile on FreeBSD, so that target uses a no-op
//! implementation that keeps this crate's API but installs no handlers and
//! writes no minidumps.

#[cfg(not(target_os = "freebsd"))]
mod supported;
#[cfg(not(target_os = "freebsd"))]
pub use supported::*;

#[cfg(target_os = "freebsd")]
mod freebsd;
#[cfg(target_os = "freebsd")]
pub use freebsd::*;
