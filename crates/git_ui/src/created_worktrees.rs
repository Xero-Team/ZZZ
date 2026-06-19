use std::{
    future::Future,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context as _, Result};
use db::kvp::KeyValueStore;
use gpui::{App, AsyncApp, Entity};
use project::git_store::Repository;
use remote::{RemoteConnectionOptions, remote_connection_identity};
use serde::{Deserialize, Serialize};
use util::ResultExt as _;

const NAMESPACE: &str = "created_git_worktrees";

#[derive(Serialize, Deserialize)]
struct CreatedWorktreeRecord {
    created_at_seconds: u64,
    created_at_subsec_nanos: u32,
}

fn record_key(worktree_path: &Path, remote: Option<&RemoteConnectionOptions>) -> String {
    let host = match remote {
        None => "local".to_string(),
        Some(options) => remote_connection_identity(options).persistence_key(),
    };
    format!("{host}\n{}", worktree_path.display())
}

pub fn record_created_worktree(
    worktree_path: &Path,
    remote: Option<&RemoteConnectionOptions>,
    created_at: SystemTime,
    cx: &App,
) -> impl Future<Output = Result<()>> + use<> {
    let store = KeyValueStore::global(cx);
    let key = record_key(worktree_path, remote);
    let value = created_at
        .duration_since(UNIX_EPOCH)
        .context("worktree creation time predates the unix epoch")
        .and_then(|duration| {
            serde_json::to_string(&CreatedWorktreeRecord {
                created_at_seconds: duration.as_secs(),
                created_at_subsec_nanos: duration.subsec_nanos(),
            })
            .context("failed to serialize created worktree record")
        });
    async move { store.scoped(NAMESPACE).write(key, value?).await }
}

pub fn recorded_created_at(
    worktree_path: &Path,
    remote: Option<&RemoteConnectionOptions>,
    cx: &App,
) -> Option<SystemTime> {
    let store = KeyValueStore::global(cx);
    let value = store
        .scoped(NAMESPACE)
        .read(&record_key(worktree_path, remote))
        .log_err()??;
    let record: CreatedWorktreeRecord = serde_json::from_str(&value).log_err()?;
    Some(UNIX_EPOCH + Duration::new(record.created_at_seconds, record.created_at_subsec_nanos))
}

pub async fn record_created_worktree_for_repo(
    repo: &Entity<Repository>,
    worktree_path: &Path,
    remote: Option<&RemoteConnectionOptions>,
    cx: &mut AsyncApp,
) {
    let receiver = repo.update(cx, |repo, _cx| {
        repo.worktree_created_at(worktree_path.to_path_buf())
    });
    let created_at = match receiver.await {
        Ok(Ok(Some(created_at))) => created_at,
        Ok(Ok(None)) => {
            log::warn!(
                "Newly created worktree {} not found on disk; it won't be eligible for automatic archival",
                worktree_path.display()
            );
            return;
        }
        Ok(Err(error)) => {
            log::warn!(
                "Couldn't determine creation time for worktree {}; it won't be eligible for automatic archival: {error:#}",
                worktree_path.display()
            );
            return;
        }
        Err(_) => {
            log::warn!(
                "Worktree creation time lookup was canceled for {}",
                worktree_path.display()
            );
            return;
        }
    };
    let record = cx.update(|cx| record_created_worktree(worktree_path, remote, created_at, cx));
    if let Err(error) = record.await {
        log::warn!(
            "Failed to record created worktree {}: {error:#}",
            worktree_path.display()
        );
    }
}

pub fn forget_created_worktree(
    worktree_path: &Path,
    remote: Option<&RemoteConnectionOptions>,
    cx: &App,
) -> impl Future<Output = Result<()>> + use<> {
    let store = KeyValueStore::global(cx);
    let key = record_key(worktree_path, remote);
    async move { store.scoped(NAMESPACE).delete(key).await }
}
