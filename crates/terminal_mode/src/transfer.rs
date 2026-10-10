//! Cross-host file copy via a local relay (R6).
//!
//! Each session's remote worktree exposes a VFS provider. We wrap those
//! providers in [`VfsSnapshot`]s registered with a self-built [`VfsManager`]
//! under assigned mount ids, then copy through
//! [`VfsManager::copy_path`], which streams source `read_at` -> local buffer ->
//! target `write_at` with SHA-256 verification and an atomic rename.

use std::path::Path;

use anyhow::{Context as _, anyhow};
use gpui::AsyncApp;
use util::rel_path::RelPath;
use vfs::{
    CollisionPolicy, MountId, SnapshotBudgets, VfsManager, VfsPath, VfsSnapshot,
    VfsTransferOptions, VfsTransferOutcome, VfsTransferProgressCallback,
};
use worktree::Worktree;

/// A unique mount id per session, so two hosts cannot collide in the shared
/// transfer manager even though each remote server starts its ids at 1.
pub fn mount_id_for_session(session_id: usize) -> MountId {
    MountId::new(session_id as u64 + 1)
}

/// Registers the session worktree's remote VFS provider with `manager`,
/// idempotently. Returns the registered snapshot.
pub async fn ensure_session_snapshot(
    manager: &VfsManager,
    mount_id: MountId,
    worktree: &gpui::Entity<Worktree>,
    cx: &mut AsyncApp,
) -> anyhow::Result<VfsSnapshot> {
    if let Ok(snapshot) = manager.snapshot(mount_id) {
        return Ok(snapshot);
    }

    let provider_task = worktree
        .read_with(cx, |worktree, cx| worktree.remote_vfs_provider(cx))
        .context("session worktree is not a remote worktree")?;
    let provider = provider_task
        .await
        .map_err(|error| anyhow!("failed to connect the remote VFS provider: {error:?}"))?;

    let snapshot = VfsSnapshot::new(mount_id, provider, SnapshotBudgets::default());
    manager
        .register(snapshot.clone())
        .map_err(|error| anyhow!("failed to register the session VFS snapshot: {error:?}"))?;
    Ok(snapshot)
}

fn session_vfs_path(
    worktree: &gpui::Entity<Worktree>,
    mount_id: MountId,
    relative_path: &RelPath,
    cx: &AsyncApp,
) -> anyhow::Result<VfsPath> {
    let worktree_path = worktree
        .read_with(cx, |worktree, _| worktree.vfs_path_for_path(relative_path))
        .context("failed to resolve the session path")?;
    Ok(VfsPath::new(
        mount_id,
        worktree_path.provider_path().clone(),
    ))
}

/// Copies `source_relative_path` from the source session into the target
/// session's home root, relaying through the local process (R6).
pub async fn copy_between_sessions(
    manager: &VfsManager,
    source_worktree: &gpui::Entity<Worktree>,
    source_mount_id: MountId,
    source_relative_path: &RelPath,
    target_worktree: &gpui::Entity<Worktree>,
    target_mount_id: MountId,
    progress: Option<VfsTransferProgressCallback>,
    cx: &mut AsyncApp,
) -> anyhow::Result<VfsTransferOutcome> {
    ensure_session_snapshot(manager, source_mount_id, source_worktree, cx).await?;
    ensure_session_snapshot(manager, target_mount_id, target_worktree, cx).await?;

    let source_path = session_vfs_path(source_worktree, source_mount_id, source_relative_path, cx)?;
    let file_name = source_relative_path
        .file_name()
        .context("source path has no file name")?;
    let target_path_style = target_worktree.read_with(cx, |worktree, _| worktree.path_style());
    let target_relative_path =
        RelPath::new(Path::new(file_name), target_path_style).context("invalid target path")?;
    let target_path =
        session_vfs_path(target_worktree, target_mount_id, &target_relative_path, cx)?;

    let options = VfsTransferOptions {
        collision: CollisionPolicy::Fail,
        progress,
        ..Default::default()
    };
    manager
        .copy_path(&source_path, &target_path, options)
        .await
        .map_err(|error| anyhow!("cross-host copy failed: {error:?}"))
}
