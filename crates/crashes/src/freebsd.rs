//! No-op crash reporting for FreeBSD.
//!
//! The `crash-handler`/`minidumper`/`minidump-writer` stack only ships Linux,
//! Windows, and macOS backends, so it is not compiled for FreeBSD (see the
//! target-gated dependencies in `Cargo.toml`). This module keeps the `crashes`
//! public API used by the call sites, but installs no signal handlers and
//! writes no minidumps. Crash reporting is opt-in and local-only anyway, so
//! FreeBSD loses only the optional local minidump, not any data or behavior.

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use std::panic::PanicHookInfo;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use system_specs::GpuSpecs;

pub static REQUESTED_MINIDUMP: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct InitCrashHandler {
    pub session_id: String,
    pub zzz_version: String,
    pub binary: String,
    pub release_channel: String,
    pub commit_sha: String,
}

/// Crash reporting has no FreeBSD backend, so this deliberately does nothing.
pub fn init<F: Future<Output = ()> + Send + Sync + 'static>(
    _crash_init: InitCrashHandler,
    _spawn: impl FnOnce(BoxFuture<'static, ()>),
    _wait_timer: impl (Fn(Duration) -> F) + Send + Sync + 'static,
) {
}

pub fn set_gpu_info(_specs: GpuSpecs) {}

pub fn panic_hook(_info: &PanicHookInfo<'_>) {}

pub fn crash_server(_socket: &Path) {}
