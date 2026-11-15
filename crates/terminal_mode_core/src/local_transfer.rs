//! Local filesystem transfers (upload/download) for terminal mode.
//!
//! Each session's remote worktree exposes a VFS provider. We register a
//! [`fs::LocalProvider`] rooted at the local filesystem root with the session
//! transfer manager under a reserved mount id, then reuse
//! [`VfsManager::copy_path`] to stream bytes in either direction. Using the
//! shared VFS transfer engine gives us recursive directory copies, progress
//! callbacks and SHA-256 verification for free.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, anyhow};
use fs::{Fs, LocalProvider};
use gpui::{AsyncApp, Entity};
use util::rel_path::RelPath;
use vfs::{
    CollisionPolicy, MountId, NativePath, SnapshotBudgets, VfsManager, VfsPath, VfsSnapshot,
    VfsTransferOptions, VfsTransferOutcome, VfsTransferProgressCallback,
};
use worktree::Worktree;

use crate::transfer::{ensure_session_snapshot, session_vfs_path};

/// Reserved mount id for the local filesystem provider shared by all sessions.
///
/// Session mounts use `session_id + 1`, so the maximum value can never collide
/// with a real session mount.
pub const LOCAL_MOUNT_ID: MountId = MountId::new(u64::MAX);

fn local_root() -> Arc<Path> {
    if cfg!(windows) {
        Arc::from(PathBuf::from("C:\\"))
    } else {
        Arc::from(PathBuf::from("/"))
    }
}

/// Registers the local filesystem provider with `manager`, idempotently.
pub async fn ensure_local_snapshot(
    manager: &VfsManager,
    fs: Arc<dyn Fs>,
) -> anyhow::Result<VfsSnapshot> {
    if let Ok(snapshot) = manager.snapshot(LOCAL_MOUNT_ID) {
        return Ok(snapshot);
    }

    let provider = LocalProvider::new("terminal-mode-local", local_root(), fs)
        .await
        .map_err(|error| anyhow!("failed to create the local VFS provider: {error:?}"))?;
    let snapshot = VfsSnapshot::new(
        LOCAL_MOUNT_ID,
        Arc::new(provider),
        SnapshotBudgets::default(),
    );
    manager
        .register(snapshot.clone())
        .map_err(|error| anyhow!("failed to register the local VFS snapshot: {error:?}"))?;
    Ok(snapshot)
}

/// Encodes an absolute local path as a VFS path in the local provider.
pub fn local_vfs_path(path: &Path) -> anyhow::Result<VfsPath> {
    let native = NativePath::from_local_path(path)
        .with_context(|| format!("failed to encode the local path {path:?}"))?;
    Ok(VfsPath::new(LOCAL_MOUNT_ID, native.provider_path().clone()))
}

fn transfer_options(progress: Option<VfsTransferProgressCallback>) -> VfsTransferOptions {
    VfsTransferOptions {
        collision: CollisionPolicy::Replace,
        progress,
        ..Default::default()
    }
}

/// Copies a remote entry from `worktree` into `destination_directory` on the
/// local filesystem, preserving the entry's file name.
#[allow(clippy::too_many_arguments)]
pub async fn download_from_session(
    manager: &VfsManager,
    fs: Arc<dyn Fs>,
    worktree: &Entity<Worktree>,
    session_mount_id: MountId,
    relative_path: &RelPath,
    destination_directory: &Path,
    progress: Option<VfsTransferProgressCallback>,
    cx: &mut AsyncApp,
) -> anyhow::Result<VfsTransferOutcome> {
    ensure_session_snapshot(manager, session_mount_id, worktree, cx).await?;
    ensure_local_snapshot(manager, fs).await?;

    let source_path = session_vfs_path(worktree, session_mount_id, relative_path, cx)?;
    let file_name = relative_path
        .file_name()
        .context("the selected remote path has no file name")?;
    let destination_path = destination_directory.join(file_name);
    let target_path = local_vfs_path(&destination_path)?;

    manager
        .copy_path(&source_path, &target_path, transfer_options(progress))
        .await
        .map_err(|error| anyhow!("download failed: {error}"))
}

/// Copies one or more local files into `target_relative_directory` on the
/// remote `worktree`.
#[allow(clippy::too_many_arguments)]
pub async fn upload_to_session(
    manager: &VfsManager,
    fs: Arc<dyn Fs>,
    worktree: &Entity<Worktree>,
    session_mount_id: MountId,
    source_paths: &[PathBuf],
    target_relative_directory: &RelPath,
    progress: Option<VfsTransferProgressCallback>,
    cx: &mut AsyncApp,
) -> anyhow::Result<Vec<VfsTransferOutcome>> {
    ensure_session_snapshot(manager, session_mount_id, worktree, cx).await?;
    ensure_local_snapshot(manager, fs).await?;

    let path_style = worktree.read_with(cx, |worktree, _| worktree.path_style());
    let mut outcomes = Vec::new();
    for source_path in source_paths {
        let file_name = source_path
            .file_name()
            .and_then(|name| name.to_str())
            .context("the selected local path has no usable file name")?;
        let target_relative_path = target_relative_directory
            .join(RelPath::new(Path::new(file_name), path_style)?.as_ref());
        let target_path = session_vfs_path(worktree, session_mount_id, &target_relative_path, cx)?;
        let source_vfs_path = local_vfs_path(source_path)?;

        let outcome = manager
            .copy_path(
                &source_vfs_path,
                &target_path,
                transfer_options(progress.clone()),
            )
            .await
            .map_err(|error| anyhow!("upload failed: {error}"))?;
        outcomes.push(outcome);
    }
    Ok(outcomes)
}
