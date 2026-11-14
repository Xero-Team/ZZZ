//! Fossil backend for the repository abstraction.
//!
//! Absorbed from fzed (https://github.com/pekeler/fzed), commit
//! 04a2955e19dec5f459566657c0767223b49a5b9c, file `crates/git/src/fossil.rs`.
//! Copyright the fzed contributors.
//!
//! This file is distributed under the GNU General Public License v3.0 or later,
//! the same terms as the rest of the ZZZ `git` crate. See `NOTICE` at the
//! repository root for the upstream attribution.
//!
//! ZZZ keeps this backend deliberately read-only: it detects a Fossil checkout,
//! opens it, and reports status and diffs. Every Fossil check-in, sync, stash,
//! and history operation fails with an explicit "not supported yet" error, so
//! the Git-shaped UI never silently pretends to have changed the repository.

use crate::{
    Oid, RunHook,
    blame::Blame,
    repository::{
        Branch, BranchesScanResult, CommitDataReader, CommitDetails, CommitDiff, CommitOptions,
        CreateWorktreeTarget, DiffType, FetchOptions, GitCommitTemplate, GitRepository,
        GitRepositoryCheckpoint, InitialGraphCommitData, LogOrder, LogSource, PushOptions, Remote,
        RemoteCommandOutput, RepoPath, ResetMode, SearchCommitArgs, Worktree,
    },
    stash::GitStash,
    status::{
        DiffStat, DiffTreeType, FileStatus, GitDiffStat, GitStatus, StatusCode, TreeDiff,
        UnmergedStatus, UnmergedStatusCode,
    },
};
use anyhow::{Context as _, Result, anyhow};
use async_channel::Sender;
use collections::HashMap;
use futures::{FutureExt as _, future::BoxFuture};
use gpui::{AsyncApp, BackgroundExecutor, SharedString, Task};
use rope::Rope;
use smol::lock::Mutex as AsyncMutex;
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::ExitStatus,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::SystemTime,
};
use text::LineEnding;
use time::PrimitiveDateTime;
use util::{command::new_command, paths::PathStyle};

pub const FOSSIL_BINARY_NAME: &str = "fossil";
pub const FOSSIL_BINARY_NOT_FOUND_MESSAGE: &str = "Fossil executable not found. Install Fossil and make sure the `fossil` command is available on PATH.";

/// Fossil checkout metadata files. A checkout has exactly one of these in its
/// root; the older name `_FOSSIL_` only appears in checkouts created before
/// Fossil 2.1.
pub const FOSSIL_CHECKOUT_DB_NAMES: [&str; 2] = [".fslckout", "_FOSSIL_"];

/// Fossil support is opt-in: the worktree scanner and the filesystem repository
/// opener both ask this before doing anything Fossil-specific, so Git-only
/// projects pay one cached environment lookup and no extra filesystem probes.
pub const FOSSIL_SUPPORT_ENV_VAR: &str = "ZZZ_FOSSIL";

pub fn fossil_support_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| match std::env::var(FOSSIL_SUPPORT_ENV_VAR) {
        Ok(value) => !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "false" | "no" | "off"
        ),
        Err(_) => false,
    })
}

pub fn is_fossil_checkout_db_path(path: &Path) -> bool {
    path.file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|file_name| FOSSIL_CHECKOUT_DB_NAMES.contains(&file_name))
}

pub fn fossil_binary_not_found_error() -> anyhow::Error {
    anyhow!(FOSSIL_BINARY_NOT_FOUND_MESSAGE)
}

pub fn resolve_fossil_binary(
    search_paths: Option<&str>,
    working_directory: &Path,
) -> Result<PathBuf> {
    if let Some(path) = search_paths
        .filter(|paths| !paths.is_empty())
        .and_then(|search_paths| {
            which::which_in(FOSSIL_BINARY_NAME, Some(search_paths), working_directory).ok()
        })
    {
        return Ok(path);
    }

    if let Ok(path) = which::which(FOSSIL_BINARY_NAME) {
        return Ok(path);
    }

    #[cfg(target_os = "macos")]
    {
        for path in common_macos_fossil_paths() {
            if path.is_file() {
                return Ok(path);
            }
        }
    }

    Err(fossil_binary_not_found_error())
}

#[cfg(target_os = "macos")]
fn common_macos_fossil_paths() -> impl IntoIterator<Item = PathBuf> {
    [
        "/opt/homebrew/bin/fossil",
        "/usr/local/bin/fossil",
        "/opt/fossil/bin/fossil",
        "/opt/local/bin/fossil",
        "/sw/bin/fossil",
    ]
    .map(PathBuf::from)
}

pub struct FossilRepository {
    checkout_db_path: PathBuf,
    work_directory: PathBuf,
    fossil_binary_path: PathBuf,
    executor: BackgroundExecutor,
    is_trusted: Arc<AtomicBool>,
    command_lock: Arc<AsyncMutex<()>>,
}

impl FossilRepository {
    pub fn new(
        checkout_db_path: &Path,
        fossil_binary_path: Option<PathBuf>,
        executor: BackgroundExecutor,
    ) -> Result<Self> {
        let work_directory = checkout_db_path
            .parent()
            .context("Fossil checkout database has no parent directory")?
            .to_path_buf();
        let fossil_binary_path = match fossil_binary_path {
            Some(path) => path,
            None => resolve_fossil_binary(None, &work_directory)?,
        };
        log::info!(
            "opening Fossil repository at {checkout_db_path:?} using fossil binary {fossil_binary_path:?}"
        );
        Ok(Self {
            checkout_db_path: checkout_db_path.to_path_buf(),
            work_directory,
            fossil_binary_path,
            executor,
            is_trusted: Arc::new(AtomicBool::new(false)),
            command_lock: Arc::default(),
        })
    }

    fn fossil_binary(&self) -> FossilBinary {
        FossilBinary::new(
            self.fossil_binary_path.clone(),
            self.work_directory.clone(),
            self.command_lock.clone(),
        )
    }

    fn unsupported<T: Send + 'static>(operation: &'static str) -> BoxFuture<'static, Result<T>> {
        async move { Err(anyhow!("Fossil backend does not support {operation} yet")) }.boxed()
    }
}

impl GitRepository for FossilRepository {
    fn reload_index(&self) {
        // Fossil has no index to reload; the checkout database is read per command.
    }

    fn load_index_text(&self, _path: RepoPath) -> BoxFuture<'_, Option<String>> {
        async move { None }.boxed()
    }

    fn load_committed_text(&self, path: RepoPath) -> BoxFuture<'_, Option<String>> {
        let fossil = self.fossil_binary();
        self.executor
            .spawn(async move {
                fossil
                    .run_raw(&[
                        OsString::from("cat"),
                        path.as_std_path().as_os_str().to_owned(),
                    ])
                    .await
                    .ok()
            })
            .boxed()
    }

    fn load_blob_content(&self, _oid: Oid) -> BoxFuture<'_, Result<String>> {
        Self::unsupported("loading blobs by Git object ID")
    }

    fn set_index_text(
        &self,
        _path: RepoPath,
        _content: Option<String>,
        _env: Arc<HashMap<String, String>>,
        _is_executable: bool,
    ) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("writing index entries")
    }

    fn remote_urls(&self) -> BoxFuture<'_, HashMap<String, String>> {
        let fossil = self.fossil_binary();
        self.executor
            .spawn(async move {
                let mut remote_urls = fossil
                    .run(&["remote", "list"])
                    .await
                    .map(|output| {
                        parse_fossil_remote_list(&output)
                            .into_iter()
                            .map(|remote| (remote.name, remote.url))
                            .collect::<HashMap<_, _>>()
                    })
                    .unwrap_or_default();

                if !remote_urls.contains_key("default")
                    && let Ok(output) = fossil.run(&["remote"]).await
                    && let Some(default_remote) = parse_fossil_default_remote(&output)
                {
                    remote_urls.insert("default".to_string(), default_remote);
                }

                remote_urls
            })
            .boxed()
    }

    fn revparse_batch(&self, revs: Vec<String>) -> BoxFuture<'_, Result<Vec<Option<String>>>> {
        let fossil = self.fossil_binary();
        self.executor
            .spawn(async move {
                let info = parse_fossil_info(&fossil.run(&["info"]).await?);
                Ok(revs
                    .into_iter()
                    .map(|rev| match rev.as_str() {
                        "HEAD" | "checkout" | "current" => info.checkout.clone(),
                        _ => None,
                    })
                    .collect())
            })
            .boxed()
    }

    fn merge_message(&self) -> BoxFuture<'_, Option<String>> {
        async move { None }.boxed()
    }

    fn status(&self, path_prefixes: &[RepoPath]) -> Task<Result<GitStatus>> {
        let fossil = self.fossil_binary();
        let path_prefixes = scoped_fossil_prefixes(path_prefixes);
        self.executor.spawn(async move {
            let has_path_prefixes = !path_prefixes.is_empty();
            let output = fossil
                .run(&fossil_changes_args(path_prefixes))
                .await
                .context("running `fossil changes`")?;
            let mut changes = parse_fossil_changes_with_kind(&output);
            if has_path_prefixes
                && changes
                    .iter()
                    .any(|change| change.kind == FossilChangeKind::Extra)
            {
                let output = fossil.run(&fossil_changes_args(Vec::new())).await?;
                changes =
                    filter_scoped_fossil_extras(changes, parse_fossil_changes_with_kind(&output));
            }
            Ok(fossil_changes_to_status(changes))
        })
    }

    fn diff_tree(&self, _request: DiffTreeType) -> BoxFuture<'_, Result<TreeDiff>> {
        async move {
            Ok(TreeDiff {
                entries: HashMap::default(),
            })
        }
        .boxed()
    }

    fn stash_entries(&self) -> BoxFuture<'static, Result<GitStash>> {
        // Fossil stashes live in the checkout database and are not part of this
        // read-only slice, so the panel shows no stash entries instead of an error.
        async move { Ok(GitStash::default()) }.boxed()
    }

    fn branches(&self) -> BoxFuture<'_, Result<BranchesScanResult>> {
        let fossil = self.fossil_binary();
        self.executor
            .spawn(async move {
                let info = parse_fossil_info(
                    &fossil
                        .run(&["info"])
                        .await
                        .context("reading Fossil checkout info")?,
                );
                let checkout_info = match info.checkout.as_deref() {
                    Some(checkout) => match fossil_commit_info(&fossil, checkout).await {
                        Ok(info) => Some(info),
                        Err(error) => {
                            log::warn!("failed to load Fossil check-in summary: {error:#}");
                            None
                        }
                    },
                    None => None,
                };
                let current = fossil
                    .run(&["branch", "current"])
                    .await
                    .ok()
                    .or_else(|| checkout_info.as_ref().and_then(|info| info.branch.clone()));
                let list = fossil
                    .run(&["branch", "list", "--all"])
                    .await
                    .unwrap_or_default();
                let current_commit = checkout_info.as_ref().map(commit_summary_from_info);
                let mut branches = Vec::new();
                for line in list.lines() {
                    if let Some(name) = parse_fossil_branch_list_line(line) {
                        let is_head = current.as_deref() == Some(name.as_str());
                        branches.push(Branch {
                            is_head,
                            ref_name: SharedString::from(format!("refs/heads/{name}")),
                            upstream: None,
                            most_recent_commit: is_head.then(|| current_commit.clone()).flatten(),
                        });
                    }
                }
                if branches.is_empty()
                    && let Some(current) = current
                {
                    branches.push(Branch {
                        is_head: true,
                        ref_name: SharedString::from(format!("refs/heads/{current}")),
                        upstream: None,
                        most_recent_commit: current_commit,
                    });
                }
                Ok(BranchesScanResult::from(branches))
            })
            .boxed()
    }

    fn change_branch(&self, _name: String) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("switching branches")
    }

    fn create_branch(
        &self,
        _name: String,
        _base_branch: Option<String>,
    ) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("creating branches")
    }

    fn rename_branch(&self, _branch: String, _new_name: String) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("renaming branches")
    }

    fn delete_branch(
        &self,
        _is_remote: bool,
        _name: String,
        _force: bool,
    ) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("deleting branches")
    }

    fn worktrees(&self) -> BoxFuture<'_, Result<Vec<Worktree>>> {
        let fossil = self.fossil_binary();
        let work_directory = self.work_directory.clone();
        self.executor
            .spawn(async move {
                let info = parse_fossil_info(&fossil.run(&["info"]).await?);
                let sha = info.checkout.unwrap_or_default();
                let branch = fossil.run(&["branch", "current"]).await.ok();
                Ok(vec![Worktree {
                    path: work_directory,
                    ref_name: branch
                        .map(|branch| SharedString::from(format!("refs/heads/{branch}"))),
                    sha: SharedString::from(sha),
                    is_main: true,
                    is_bare: false,
                }])
            })
            .boxed()
    }

    fn worktree_created_at(
        &self,
        worktree_path: PathBuf,
    ) -> BoxFuture<'_, Result<Option<SystemTime>>> {
        self.executor
            .spawn(async move {
                let metadata = match std::fs::metadata(&worktree_path) {
                    Ok(metadata) => metadata,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        return Ok(None);
                    }
                    Err(error) => {
                        return Err(error).with_context(|| {
                            format!("failed to stat {}", worktree_path.display())
                        });
                    }
                };

                metadata.created().map(Some).with_context(|| {
                    format!("creation time unavailable for {}", worktree_path.display())
                })
            })
            .boxed()
    }

    fn create_worktree(
        &self,
        _target: CreateWorktreeTarget,
        _path: PathBuf,
    ) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("creating Fossil checkouts")
    }

    fn checkout_branch_in_worktree(
        &self,
        _branch_name: String,
        _worktree_path: PathBuf,
        _create: bool,
    ) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("switching branches in a Fossil checkout")
    }

    fn remove_worktree(&self, _path: PathBuf, _force: bool) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("removing Fossil checkouts")
    }

    fn rename_worktree(&self, _old_path: PathBuf, _new_path: PathBuf) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("renaming Fossil checkouts")
    }

    fn reset(
        &self,
        _commit: String,
        _mode: ResetMode,
        _env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("reset")
    }

    fn checkout_files(
        &self,
        commit: String,
        paths: Vec<RepoPath>,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        let fossil = self.fossil_binary();
        self.executor
            .spawn(async move {
                if paths.is_empty() {
                    return Ok(());
                }

                let mut args = vec![OsString::from("revert")];
                if commit != "HEAD" {
                    args.push(OsString::from("--revision"));
                    args.push(OsString::from(commit));
                }
                args.extend(repo_paths_to_args(paths));
                fossil.run_with_env(&args, env.as_ref()).await?;
                Ok(())
            })
            .boxed()
    }

    fn show(&self, commit: String) -> BoxFuture<'_, Result<CommitDetails>> {
        let fossil = self.fossil_binary();
        self.executor
            .spawn(async move {
                let commit = match commit.as_str() {
                    "HEAD" | "checkout" | "current" => parse_fossil_info(
                        &fossil
                            .run(&["info"])
                            .await
                            .context("reading Fossil checkout info")?,
                    )
                    .checkout
                    .context("Fossil checkout hash is unavailable")?,
                    _ => commit,
                };
                let info = fossil_commit_info(&fossil, &commit).await?;
                Ok(CommitDetails {
                    sha: SharedString::from(info.hash),
                    message: SharedString::from(info.comment),
                    commit_timestamp: info.timestamp,
                    author_name: SharedString::from(info.user.unwrap_or_default()),
                    ..Default::default()
                })
            })
            .boxed()
    }

    fn load_commit(&self, _commit: String, _cx: AsyncApp) -> BoxFuture<'_, Result<CommitDiff>> {
        Self::unsupported("loading check-in diffs")
    }

    fn blame(
        &self,
        _path: RepoPath,
        _content: Rope,
        _line_ending: LineEnding,
    ) -> BoxFuture<'_, Result<Blame>> {
        Self::unsupported("blame")
    }

    fn path(&self) -> PathBuf {
        self.checkout_db_path.clone()
    }

    fn main_repository_path(&self) -> PathBuf {
        self.checkout_db_path.clone()
    }

    fn stage_paths(
        &self,
        _paths: Vec<RepoPath>,
        _env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("Git-style staging")
    }

    fn unstage_paths(
        &self,
        _paths: Vec<RepoPath>,
        _env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("Git-style unstaging")
    }

    fn run_hook(
        &self,
        _hook: RunHook,
        _env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        async move { Ok(()) }.boxed()
    }

    fn commit(
        &self,
        _message: SharedString,
        _name_and_email: Option<(SharedString, SharedString)>,
        _options: CommitOptions,
        _askpass: askpass::AskPassDelegate,
        _env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("check-ins")
    }

    fn stash_paths(
        &self,
        _paths: Vec<RepoPath>,
        _message: Option<String>,
        _env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("stashing")
    }

    fn stash_pop(
        &self,
        _index: Option<usize>,
        _env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("popping stashes")
    }

    fn stash_apply(
        &self,
        _index: Option<usize>,
        _env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("applying stashes")
    }

    fn stash_drop(
        &self,
        _index: Option<usize>,
        _env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("dropping stashes")
    }

    fn push(
        &self,
        _branch_name: String,
        _remote_branch_name: String,
        _upstream_name: String,
        _options: Option<PushOptions>,
        _askpass: askpass::AskPassDelegate,
        _env: Arc<HashMap<String, String>>,
        _cx: AsyncApp,
    ) -> BoxFuture<'_, Result<RemoteCommandOutput>> {
        Self::unsupported("pushing")
    }

    fn pull(
        &self,
        _branch_name: Option<String>,
        _upstream_name: String,
        _rebase: bool,
        _askpass: askpass::AskPassDelegate,
        _env: Arc<HashMap<String, String>>,
        _cx: AsyncApp,
    ) -> BoxFuture<'_, Result<RemoteCommandOutput>> {
        Self::unsupported("pulling")
    }

    fn fetch(
        &self,
        _fetch_options: FetchOptions,
        _askpass: Option<askpass::AskPassDelegate>,
        _env: Arc<HashMap<String, String>>,
        _cx: AsyncApp,
    ) -> BoxFuture<'_, Result<RemoteCommandOutput>> {
        Self::unsupported("fetching")
    }

    fn get_push_remote(&self, _branch: String) -> BoxFuture<'_, Result<Option<Remote>>> {
        let fossil = self.fossil_binary();
        self.executor
            .spawn(async move { default_fossil_remote(&fossil).await })
            .boxed()
    }

    fn get_branch_remote(&self, _branch: String) -> BoxFuture<'_, Result<Option<Remote>>> {
        let fossil = self.fossil_binary();
        self.executor
            .spawn(async move { default_fossil_remote(&fossil).await })
            .boxed()
    }

    fn get_all_remotes(&self) -> BoxFuture<'_, Result<Vec<Remote>>> {
        let fossil = self.fossil_binary();
        self.executor
            .spawn(async move {
                let mut remotes = parse_fossil_remote_list(&fossil.run(&["remote", "list"]).await?)
                    .into_iter()
                    .map(|remote| Remote {
                        name: SharedString::from(remote.name),
                    })
                    .collect::<Vec<_>>();

                if remotes.is_empty()
                    && parse_fossil_default_remote(&fossil.run(&["remote"]).await?).is_some()
                {
                    remotes.push(Remote {
                        name: SharedString::from("default"),
                    });
                }

                Ok(remotes)
            })
            .boxed()
    }

    fn remove_remote(&self, _name: String) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("removing remotes")
    }

    fn create_remote(&self, _name: String, _url: String) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("creating remotes")
    }

    fn check_for_pushed_commit(&self) -> BoxFuture<'_, Result<Vec<SharedString>>> {
        async move { Ok(Vec::new()) }.boxed()
    }

    fn diff(&self, diff: DiffType) -> BoxFuture<'_, Result<String>> {
        let fossil = self.fossil_binary();
        self.executor
            .spawn(async move {
                match diff {
                    DiffType::HeadToWorktree => {
                        fossil.run_raw(&["diff", "--internal", "--unified"]).await
                    }
                    DiffType::HeadToIndex => Ok(String::new()),
                    DiffType::MergeBase { .. } => Err(anyhow!(
                        "Fossil backend does not support merge-base diffs yet"
                    )),
                }
            })
            .boxed()
    }

    fn diff_stat(&self, path_prefixes: &[RepoPath]) -> BoxFuture<'static, Result<GitDiffStat>> {
        let fossil = self.fossil_binary();
        let path_prefixes = scoped_fossil_prefixes(path_prefixes);
        self.executor
            .spawn(async move {
                let mut args = vec![OsString::from("diff"), OsString::from("--numstat")];
                for prefix in &path_prefixes {
                    args.push(prefix.as_std_path().as_os_str().to_owned());
                }
                let output = match fossil.run(&args).await {
                    Ok(output) => output,
                    Err(error)
                        if !path_prefixes.is_empty()
                            && error
                                .downcast_ref::<FossilBinaryCommandError>()
                                .is_some_and(|error| error.stderr.starts_with("not found:")) =>
                    {
                        // Fossil rejects scoped diffs of deleted untracked files, which
                        // would otherwise prevent their stale statuses from being removed.
                        let output = fossil.run(&["diff", "--numstat"]).await?;
                        let stats = parse_fossil_numstat(&output);
                        return Ok(GitDiffStat {
                            entries: stats
                                .entries
                                .iter()
                                .filter(|(path, _)| {
                                    path_prefixes.iter().any(|prefix| path.starts_with(prefix))
                                })
                                .cloned()
                                .collect(),
                        });
                    }
                    Err(error) => return Err(error),
                };
                Ok(parse_fossil_numstat(&output))
            })
            .boxed()
    }

    fn checkpoint(&self) -> BoxFuture<'static, Result<GitRepositoryCheckpoint>> {
        Self::unsupported("checkpoints")
    }

    fn restore_checkpoint(
        &self,
        _checkpoint: GitRepositoryCheckpoint,
    ) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("restoring checkpoints")
    }

    fn create_archive_checkpoint(&self) -> BoxFuture<'_, Result<(String, String)>> {
        Self::unsupported("archive checkpoints")
    }

    fn restore_archive_checkpoint(
        &self,
        _staged_sha: String,
        _unstaged_sha: String,
    ) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("restoring archive checkpoints")
    }

    fn compare_checkpoints(
        &self,
        _left: GitRepositoryCheckpoint,
        _right: GitRepositoryCheckpoint,
    ) -> BoxFuture<'_, Result<bool>> {
        Self::unsupported("comparing checkpoints")
    }

    fn diff_checkpoints(
        &self,
        _base_checkpoint: GitRepositoryCheckpoint,
        _target_checkpoint: GitRepositoryCheckpoint,
    ) -> BoxFuture<'_, Result<String>> {
        Self::unsupported("diffing checkpoints")
    }

    fn load_commit_template(&self) -> BoxFuture<'_, Result<Option<GitCommitTemplate>>> {
        async move { Ok(None) }.boxed()
    }

    fn default_branch(
        &self,
        _include_remote_name: bool,
    ) -> BoxFuture<'_, Result<Option<SharedString>>> {
        async move { Ok(Some(SharedString::from("trunk"))) }.boxed()
    }

    fn initial_graph_data(
        &self,
        _log_source: LogSource,
        _log_order: LogOrder,
        _request_tx: Sender<Vec<Arc<InitialGraphCommitData>>>,
    ) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("the commit graph")
    }

    fn search_commits(
        &self,
        _log_source: LogSource,
        _search_args: SearchCommitArgs,
        _request_tx: Sender<Oid>,
    ) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("searching history")
    }

    fn commit_data_reader(&self) -> Result<CommitDataReader> {
        Err(anyhow!(
            "Fossil backend does not support loading commit data yet"
        ))
    }

    fn create_ref(&self, _ref_name: String, _commit: String) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("creating tags")
    }

    fn update_ref(&self, _ref_name: String, _commit: String) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("updating refs")
    }

    fn delete_ref(&self, _ref_name: String) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("deleting refs")
    }

    fn repair_worktrees(&self) -> BoxFuture<'_, Result<()>> {
        Self::unsupported("repairing checkouts")
    }

    fn set_trusted(&self, trusted: bool) {
        self.is_trusted.store(trusted, Ordering::Release);
    }

    fn is_trusted(&self) -> bool {
        self.is_trusted.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug, Default)]
struct FossilInfo {
    checkout: Option<String>,
}

fn parse_fossil_info(output: &str) -> FossilInfo {
    let mut info = FossilInfo::default();
    for line in output.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if key.trim() == "checkout" {
            info.checkout = value.split_whitespace().next().map(str::to_owned);
        }
    }
    info
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct FossilCommitInfo {
    hash: String,
    parents: Vec<String>,
    branch: Option<String>,
    comment: String,
    user: Option<String>,
    timestamp: i64,
}

async fn fossil_commit_info(fossil: &FossilBinary, commit: &str) -> Result<FossilCommitInfo> {
    let output = fossil.run_raw(&["info", commit]).await?;
    parse_fossil_commit_info(&output).with_context(|| format!("parsing Fossil info for {commit}"))
}

fn commit_summary_from_info(info: &FossilCommitInfo) -> crate::repository::CommitSummary {
    crate::repository::CommitSummary {
        sha: SharedString::from(info.hash.clone()),
        subject: SharedString::from(info.comment.lines().next().unwrap_or_default().to_string()),
        commit_timestamp: info.timestamp,
        author_name: SharedString::from(info.user.clone().unwrap_or_default()),
        has_parent: !info.parents.is_empty(),
    }
}

fn parse_fossil_commit_info(output: &str) -> Result<FossilCommitInfo> {
    let mut info = FossilCommitInfo::default();
    for line in output.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "hash" | "checkout" => {
                let mut parts = value.split_whitespace();
                if let Some(hash) = parts.next() {
                    info.hash = hash.to_string();
                }
                let date = parts.collect::<Vec<_>>().join(" ");
                if let Some(timestamp) = parse_fossil_timestamp(&date) {
                    info.timestamp = timestamp;
                }
            }
            "parent" => {
                if let Some(parent) = value.split_whitespace().next() {
                    info.parents.push(parent.to_string());
                }
            }
            "tags" => {
                // Fossil lists the branch a check-in belongs to as one of its symbolic
                // tags; `sym-trunk` only marks the initial check-in.
                info.branch = value
                    .split(',')
                    .flat_map(|part| part.split_whitespace())
                    .map(str::trim)
                    .filter(|tag| !tag.is_empty() && *tag != "sym-trunk")
                    .map(str::to_string)
                    .next();
            }
            "comment" => {
                let (comment, user) = parse_fossil_comment_user(value);
                info.comment = comment.to_string();
                info.user = user.map(str::to_string);
            }
            "user" => {
                if !value.is_empty() {
                    info.user = Some(value.to_string());
                }
            }
            _ => {}
        }
    }

    anyhow::ensure!(!info.hash.is_empty(), "missing Fossil check-in hash");
    Ok(info)
}

fn parse_fossil_comment_user(value: &str) -> (&str, Option<&str>) {
    if let Some((comment, user)) = value.rsplit_once(" (user: ")
        && let Some(user) = user.strip_suffix(')')
    {
        return (comment, Some(user));
    }
    (value, None)
}

fn parse_fossil_timestamp(value: &str) -> Option<i64> {
    let value = value.trim().trim_end_matches(" UTC").trim();
    if value.is_empty() {
        return None;
    }
    let format = time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
    PrimitiveDateTime::parse(value, &format)
        .ok()
        .map(|date_time| date_time.assume_utc().unix_timestamp())
}

fn parse_fossil_branch_list_line(line: &str) -> Option<String> {
    let name = line
        .trim()
        .trim_start_matches('*')
        .trim_start_matches('#')
        .trim();
    (!name.is_empty()).then(|| name.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FossilRemote {
    name: String,
    url: String,
}

fn parse_fossil_remote_list(output: &str) -> Vec<FossilRemote> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let (name, url) = line.split_once(char::is_whitespace)?;
            let url = url.trim();
            (!url.is_empty()).then(|| FossilRemote {
                name: name.to_string(),
                url: url.to_string(),
            })
        })
        .collect()
}

fn parse_fossil_default_remote(output: &str) -> Option<String> {
    let remote = output.trim();
    (!remote.is_empty() && remote != "off").then(|| remote.to_string())
}

async fn default_fossil_remote(fossil: &FossilBinary) -> Result<Option<Remote>> {
    if parse_fossil_default_remote(&fossil.run(&["remote"]).await?).is_some() {
        Ok(Some(Remote {
            name: SharedString::from("default"),
        }))
    } else {
        Ok(
            parse_fossil_remote_list(&fossil.run(&["remote", "list"]).await?)
                .into_iter()
                .next()
                .map(|remote| Remote {
                    name: SharedString::from(remote.name),
                }),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FossilChange {
    repo_path: RepoPath,
    kind: FossilChangeKind,
    rename_source: Option<RepoPath>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FossilChangeKind {
    Extra,
    Conflict,
    Added,
    Deleted,
    Missing,
    Renamed,
    Modified,
}

impl FossilChangeKind {
    fn status(self) -> FileStatus {
        match self {
            Self::Extra => FileStatus::Untracked,
            Self::Conflict => FileStatus::Unmerged(UnmergedStatus {
                first_head: UnmergedStatusCode::Updated,
                second_head: UnmergedStatusCode::Updated,
            }),
            Self::Added => StatusCode::Added.worktree(),
            Self::Deleted | Self::Missing => StatusCode::Deleted.worktree(),
            Self::Renamed => StatusCode::Renamed.worktree(),
            Self::Modified => StatusCode::Modified.worktree(),
        }
    }
}

fn parse_fossil_changes_with_kind(output: &str) -> Vec<FossilChange> {
    let mut entries = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line == "(none)" {
            continue;
        }
        let Some((change_type, path)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let Some((rename_source, repo_path)) = parse_fossil_change_path(path.trim()) else {
            continue;
        };
        let kind = match change_type {
            "EXTRA" => FossilChangeKind::Extra,
            "CONFLICT" => FossilChangeKind::Conflict,
            "ADDED" => FossilChangeKind::Added,
            "DELETED" => FossilChangeKind::Deleted,
            "MISSING" => FossilChangeKind::Missing,
            "RENAMED" => FossilChangeKind::Renamed,
            "EXECUTABLE"
            | "META"
            | "EDITED"
            | "MERGED"
            | "UPDATED_BY_MERGE"
            | "UPDATED_BY_INTEGRATE" => FossilChangeKind::Modified,
            _ => continue,
        };
        entries.push(FossilChange {
            repo_path,
            kind,
            rename_source,
        });
    }
    entries.sort_unstable_by(|left, right| left.repo_path.cmp(&right.repo_path));
    entries.dedup_by(|left, right| left.repo_path == right.repo_path);
    entries
}

fn parse_fossil_change_path(path: &str) -> Option<(Option<RepoPath>, RepoPath)> {
    if path.is_empty() {
        return None;
    }

    let (rename_source, path) = if let Some((source, target)) = path.rsplit_once(" -> ") {
        let source = RepoPath::from_std_path(Path::new(source.trim()), PathStyle::local()).ok()?;
        (Some(source), target.trim())
    } else {
        (None, path)
    };

    let repo_path = RepoPath::from_std_path(Path::new(path), PathStyle::local()).ok()?;
    Some((rename_source, repo_path))
}

fn fossil_changes_to_status(changes: Vec<FossilChange>) -> GitStatus {
    GitStatus {
        entries: changes
            .into_iter()
            .map(|change| (change.repo_path, change.kind.status()))
            .collect::<Vec<_>>()
            .into(),
    }
}

fn filter_scoped_fossil_extras(
    scoped_changes: Vec<FossilChange>,
    unscoped_changes: Vec<FossilChange>,
) -> Vec<FossilChange> {
    let visible_extras = unscoped_changes
        .into_iter()
        .filter_map(|change| (change.kind == FossilChangeKind::Extra).then_some(change.repo_path))
        .collect::<collections::HashSet<_>>();

    scoped_changes
        .into_iter()
        .filter(|change| {
            change.kind != FossilChangeKind::Extra || visible_extras.contains(&change.repo_path)
        })
        .collect()
}

/// `GitStore` scans the whole repository by passing `.` as the path prefix, which
/// `RelPath` turns into an empty path. Fossil rejects an empty pathspec, so drop
/// those before building the command line.
fn scoped_fossil_prefixes(path_prefixes: &[RepoPath]) -> Vec<RepoPath> {
    path_prefixes
        .iter()
        .filter(|path| !path.is_empty())
        .cloned()
        .collect()
}

fn fossil_changes_args(path_prefixes: Vec<RepoPath>) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("changes"),
        OsString::from("--classify"),
        OsString::from("--differ"),
        OsString::from("--dotfiles"),
        OsString::from("--no-merge"),
        OsString::from("--rel-paths"),
    ];
    args.extend(repo_paths_to_args(path_prefixes));
    args
}

fn repo_paths_to_args(paths: Vec<RepoPath>) -> impl Iterator<Item = OsString> {
    paths
        .into_iter()
        .map(|path| path.as_std_path().as_os_str().to_owned())
}

fn parse_fossil_numstat(output: &str) -> GitDiffStat {
    let mut entries = Vec::new();
    for line in output.lines() {
        let Some((added_str, rest)) = take_token(line.trim_start()) else {
            continue;
        };
        let Some((deleted_str, path)) = take_token(rest.trim_start()) else {
            continue;
        };
        let Ok(added) = added_str.parse::<u32>() else {
            continue;
        };
        let Ok(deleted) = deleted_str.parse::<u32>() else {
            continue;
        };
        let path = path.trim_start();
        if path.is_empty() || path.starts_with("TOTAL ") {
            continue;
        }
        let Ok(path) = RepoPath::from_std_path(Path::new(path), PathStyle::local()) else {
            continue;
        };
        entries.push((path, DiffStat { added, deleted }));
    }
    entries.sort_by(|(left, _), (right, _)| left.cmp(right));
    entries.dedup_by(|(left, _), (right, _)| left == right);

    GitDiffStat {
        entries: entries.into(),
    }
}

fn take_token(input: &str) -> Option<(&str, &str)> {
    let index = input.find(char::is_whitespace)?;
    Some((&input[..index], &input[index..]))
}

#[derive(Clone)]
struct FossilBinary {
    fossil_binary_path: PathBuf,
    working_directory: PathBuf,
    command_lock: Arc<AsyncMutex<()>>,
}

impl FossilBinary {
    fn new(
        fossil_binary_path: PathBuf,
        working_directory: PathBuf,
        command_lock: Arc<AsyncMutex<()>>,
    ) -> Self {
        Self {
            fossil_binary_path,
            working_directory,
            command_lock,
        }
    }

    async fn run<S>(&self, args: &[S]) -> Result<String>
    where
        S: AsRef<OsStr>,
    {
        let mut stdout = self.run_raw(args).await?;
        if stdout.ends_with('\n') {
            stdout.pop();
        }
        Ok(stdout)
    }

    async fn run_with_env<S>(&self, args: &[S], env: &HashMap<String, String>) -> Result<String>
    where
        S: AsRef<OsStr>,
    {
        let mut stdout = self.run_raw_with_env(args, Some(env)).await?;
        if stdout.ends_with('\n') {
            stdout.pop();
        }
        Ok(stdout)
    }

    async fn run_raw<S>(&self, args: &[S]) -> Result<String>
    where
        S: AsRef<OsStr>,
    {
        self.run_raw_with_env(args, None).await
    }

    async fn run_raw_with_env<S>(
        &self,
        args: &[S],
        env: Option<&HashMap<String, String>>,
    ) -> Result<String>
    where
        S: AsRef<OsStr>,
    {
        // Fossil keeps its checkout state in a single SQLite database, so commands
        // against one checkout must not interleave.
        let _lock = self.command_lock.lock().await;
        let mut command = new_command(&self.fossil_binary_path);
        command
            .current_dir(&self.working_directory)
            .args(args.iter().map(AsRef::as_ref));
        if let Some(env) = env {
            command.envs(env.iter());
        }
        let output = command.output().await.with_context(|| {
            format!(
                "running Fossil executable at {}",
                self.fossil_binary_path.display()
            )
        })?;

        anyhow::ensure!(
            output.status.success(),
            FossilBinaryCommandError {
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                status: output.status,
            }
        );
        Ok(String::from_utf8(output.stdout)?)
    }
}

#[derive(thiserror::Error, Debug)]
#[error("Fossil command failed:\n{stdout}{stderr}\n")]
struct FossilBinaryCommandError {
    stdout: String,
    stderr: String,
    status: ExitStatus,
}

#[cfg(test)]
mod tests {
    use super::{
        FossilRepository, parse_fossil_branch_list_line, parse_fossil_changes_with_kind,
        parse_fossil_commit_info, parse_fossil_default_remote, parse_fossil_info,
        parse_fossil_numstat, parse_fossil_remote_list, resolve_fossil_binary,
    };
    use crate::{
        repository::{GitRepository, RepoPath},
        status::{FileStatus, StatusCode},
    };
    use std::path::Path;

    #[test]
    fn parses_fossil_info_checkout() {
        let info = parse_fossil_info(
            "project-name: demo\nrepository:   /tmp/repo.fossil\nlocal-root:   /tmp/checkout/\ncheckout:     abc123 2026-05-12 10:00:00 UTC\n",
        );
        assert_eq!(info.checkout.as_deref(), Some("abc123"));
    }

    #[test]
    fn parses_fossil_changes() {
        let status = super::fossil_changes_to_status(parse_fossil_changes_with_kind(
            "EDITED src/main.rs\nADDED new file.txt\nDELETED old.rs\nEXTRA scratch.txt\nCONFLICT both.rs\n",
        ));

        let lookup = |path: &str| {
            status
                .entries
                .iter()
                .find(|(repo_path, _)| repo_path == &RepoPath::new(path).unwrap())
                .map(|(_, status)| *status)
        };

        assert_eq!(lookup("src/main.rs"), Some(StatusCode::Modified.worktree()));
        assert_eq!(lookup("new file.txt"), Some(StatusCode::Added.worktree()));
        assert_eq!(lookup("old.rs"), Some(StatusCode::Deleted.worktree()));
        assert_eq!(lookup("scratch.txt"), Some(FileStatus::Untracked));
        assert!(lookup("both.rs").unwrap().is_conflicted());
    }

    #[test]
    fn filters_ignored_fossil_extras_from_scoped_status() {
        let scoped_changes = parse_fossil_changes_with_kind(
            "EXTRA ignored.txt\nEXTRA scratch.txt\nEDITED tracked.txt\n",
        );
        let unscoped_changes =
            parse_fossil_changes_with_kind("EXTRA scratch.txt\nEDITED tracked.txt\n");
        let status = super::fossil_changes_to_status(super::filter_scoped_fossil_extras(
            scoped_changes,
            unscoped_changes,
        ));

        let lookup = |path: &str| {
            status
                .entries
                .iter()
                .find(|(repo_path, _)| repo_path == &RepoPath::new(path).unwrap())
                .map(|(_, status)| *status)
        };

        assert_eq!(lookup("ignored.txt"), None);
        assert_eq!(lookup("scratch.txt"), Some(FileStatus::Untracked));
        assert_eq!(lookup("tracked.txt"), Some(StatusCode::Modified.worktree()));
    }

    #[test]
    fn parses_fossil_numstat() {
        let stats = parse_fossil_numstat(
            "  INSERTED    DELETED\n         1          2 tracked.txt\n         3          4 dir/file with spaces.txt\n         4          6 TOTAL over 2 changed files\n",
        );

        let lookup = |path: &str| {
            stats
                .entries
                .iter()
                .find(|(repo_path, _)| repo_path == &RepoPath::new(path).unwrap())
                .map(|(_, stat)| *stat)
        };

        assert_eq!(lookup("tracked.txt").unwrap().added, 1);
        assert_eq!(lookup("tracked.txt").unwrap().deleted, 2);
        assert_eq!(lookup("dir/file with spaces.txt").unwrap().added, 3);
        assert_eq!(lookup("dir/file with spaces.txt").unwrap().deleted, 4);
        assert_eq!(stats.entries.len(), 2);
    }

    #[test]
    fn recognizes_fossil_checkout_metadata_paths() {
        assert!(super::is_fossil_checkout_db_path(Path::new(
            "/tmp/checkout/.fslckout"
        )));
        assert!(super::is_fossil_checkout_db_path(Path::new(
            "/tmp/checkout/_FOSSIL_"
        )));
        assert!(!super::is_fossil_checkout_db_path(Path::new(
            "/tmp/checkout/.git"
        )));
        assert!(!super::is_fossil_checkout_db_path(Path::new(
            "/tmp/checkout/.fslckout.backup"
        )));
    }

    #[test]
    fn parses_fossil_branch_lines() {
        assert_eq!(
            parse_fossil_branch_list_line("* trunk").as_deref(),
            Some("trunk")
        );
        assert_eq!(
            parse_fossil_branch_list_line("  # private").as_deref(),
            Some("private")
        );
        assert_eq!(parse_fossil_branch_list_line("   "), None);
    }

    #[test]
    fn parses_fossil_commit_info() {
        let info = parse_fossil_commit_info(
            "hash:         1234567890abcdef1234567890abcdef12345678 2026-05-13 06:44:31 UTC\nparent:       fedcba0987654321fedcba0987654321fedcba09\ntags:         sym-trunk, feature\ncomment:      subject line (user: tester)\n",
        )
        .unwrap();

        assert_eq!(info.hash, "1234567890abcdef1234567890abcdef12345678");
        assert_eq!(info.parents, ["fedcba0987654321fedcba0987654321fedcba09"]);
        assert_eq!(info.branch.as_deref(), Some("feature"));
        assert_eq!(info.comment, "subject line");
        assert_eq!(info.user.as_deref(), Some("tester"));
        assert_eq!(info.timestamp, 1778654671);
    }

    #[test]
    fn parses_fossil_remotes() {
        assert_eq!(
            parse_fossil_remote_list(
                "default https://example.com/repo\nmirror  /tmp/repo.fossil\n"
            ),
            [
                super::FossilRemote {
                    name: "default".to_string(),
                    url: "https://example.com/repo".to_string(),
                },
                super::FossilRemote {
                    name: "mirror".to_string(),
                    url: "/tmp/repo.fossil".to_string(),
                },
            ]
        );
        assert_eq!(
            parse_fossil_default_remote("https://example.com/repo\n").as_deref(),
            Some("https://example.com/repo")
        );
        assert_eq!(parse_fossil_default_remote("off\n"), None);
    }

    #[cfg(unix)]
    #[test]
    fn resolves_fossil_binary_from_search_path() {
        use std::{io::Write as _, os::unix::fs::PermissionsExt};

        let temp_dir = tempfile::tempdir().unwrap();
        let fossil_path = temp_dir.path().join("fossil");
        {
            let mut script = std::fs::File::create(&fossil_path).unwrap();
            script.write_all(b"#!/bin/sh\nexit 0\n").unwrap();
        }
        std::fs::set_permissions(&fossil_path, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(
            resolve_fossil_binary(temp_dir.path().to_str(), temp_dir.path()).unwrap(),
            fossil_path
        );
    }

    #[cfg(unix)]
    #[gpui::test]
    async fn fossil_status_reads_changes_from_the_checkout(cx: &mut gpui::TestAppContext) {
        use std::{io::Write as _, os::unix::fs::PermissionsExt};

        cx.executor().allow_parking();

        let temp_dir = tempfile::tempdir().unwrap();
        let checkout = temp_dir.path().join("checkout");
        std::fs::create_dir(&checkout).unwrap();
        std::fs::write(checkout.join(".fslckout"), "").unwrap();

        let script_path = temp_dir.path().join("fossil");
        {
            let mut script = std::fs::File::create(&script_path).unwrap();
            script
                .write_all(
                    b"#!/bin/sh\n\
                      if [ \"$1\" = changes ]; then\n\
                        printf 'EDITED src/main.rs\\nEXTRA scratch.txt\\n'\n\
                        exit 0\n\
                      fi\n\
                      exit 1\n",
                )
                .unwrap();
        }
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();

        let repository = FossilRepository::new(
            &checkout.join(".fslckout"),
            Some(script_path),
            cx.executor(),
        )
        .unwrap();

        let status = repository.status(&[]).await.unwrap();
        let lookup = |path: &str| {
            status
                .entries
                .iter()
                .find(|(repo_path, _)| repo_path == &RepoPath::new(path).unwrap())
                .map(|(_, status)| *status)
        };

        assert_eq!(lookup("src/main.rs"), Some(StatusCode::Modified.worktree()));
        assert_eq!(lookup("scratch.txt"), Some(FileStatus::Untracked));
    }
}
