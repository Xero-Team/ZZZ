from __future__ import annotations

import subprocess
from pathlib import Path

from syncflow.models import SyncflowError


def git(*args: str, repo_root: Path) -> str:
    try:
        result = subprocess.run(
            ["git", *args],
            cwd=repo_root,
            capture_output=True,
            text=True,
            check=True,
        )
    except subprocess.CalledProcessError as error:
        message = error.stderr.strip() or error.stdout.strip() or "git command failed"
        raise SyncflowError("git_command_failed", message) from error
    return result.stdout


def git_ok(*args: str, repo_root: Path) -> bool:
    try:
        subprocess.run(
            ["git", *args],
            cwd=repo_root,
            capture_output=True,
            text=True,
            check=True,
        )
    except subprocess.CalledProcessError:
        return False
    return True


def ensure_clean_worktree(repo_root: Path, *, operation: str = "syncflow operation") -> None:
    status = git("status", "--short", repo_root=repo_root).splitlines()
    non_sync_changes = []
    for line in status:
        if not line.strip():
            continue
        path = line[3:] if len(line) > 3 else line
        normalized = path.strip()
        if normalized.startswith(".sync/"):
            continue
        non_sync_changes.append(normalized)
    if non_sync_changes:
        raise SyncflowError(
            "dirty_worktree",
            f"Working tree must be clean before {operation}",
        )


def current_branch(repo_root: Path) -> str:
    return git("rev-parse", "--abbrev-ref", "HEAD", repo_root=repo_root).strip()


def rev_parse(ref: str, repo_root: Path) -> str:
    return git("rev-parse", ref, repo_root=repo_root).strip()


def merge_base(ref_a: str, ref_b: str, repo_root: Path) -> str:
    return git("merge-base", ref_a, ref_b, repo_root=repo_root).strip()


def ref_exists(ref: str, repo_root: Path) -> bool:
    return git_ok("rev-parse", "--verify", ref, repo_root=repo_root)


def checkout(branch: str, repo_root: Path) -> None:
    git("checkout", branch, repo_root=repo_root)


def create_branch(branch: str, start_point: str, repo_root: Path) -> None:
    git("checkout", "-b", branch, start_point, repo_root=repo_root)


def fetch_branch(remote: str, branch: str, repo_root: Path) -> str:
    return git("fetch", remote, branch, repo_root=repo_root)


def merge_no_ff(base_branch: str, sync_branch: str, repo_root: Path) -> str:
    checkout(base_branch, repo_root=repo_root)
    git("merge", "--no-ff", "--no-edit", sync_branch, repo_root=repo_root)
    return rev_parse("HEAD", repo_root=repo_root)


def local_branch_exists(branch: str, repo_root: Path) -> bool:
    return git_ok("show-ref", "--verify", f"refs/heads/{branch}", repo_root=repo_root)


def remote_branch_exists(remote: str, branch: str, repo_root: Path) -> bool:
    return git_ok("ls-remote", "--exit-code", "--heads", remote, branch, repo_root=repo_root)


def delete_local_branch(branch: str, repo_root: Path) -> bool:
    if not local_branch_exists(branch, repo_root=repo_root):
        return False
    git("branch", "-d", branch, repo_root=repo_root)
    return True


def delete_remote_branch(remote: str, branch: str, repo_root: Path) -> bool:
    if not remote_branch_exists(remote, branch, repo_root=repo_root):
        return False
    git("push", remote, "--delete", branch, repo_root=repo_root)
    return True


def list_candidates(
    repo_root: Path, from_ref: str, to_ref: str
) -> list[dict[str, object]]:
    output = git(
        "log",
        "--reverse",
        "--format=%H%x1f%s",
        "--name-only",
        f"{from_ref}..{to_ref}",
        repo_root=repo_root,
    )

    commits: list[dict[str, object]] = []
    current: dict[str, object] | None = None
    for raw_line in output.splitlines():
        if "\x1f" in raw_line:
            sha, title = raw_line.split("\x1f", 1)
            current = {"sha": sha, "title": title, "paths": []}
            commits.append(current)
            continue

        if not raw_line.strip() or current is None:
            continue

        paths = current["paths"]
        assert isinstance(paths, list)
        if raw_line not in paths:
            paths.append(raw_line)

    for commit in commits:
        paths = commit["paths"]
        assert isinstance(paths, list)
        commit["path_summary"] = paths[:10]
    filtered: list[dict[str, object]] = []
    for commit in commits:
        title = str(commit["title"])
        paths = commit["paths"]
        assert isinstance(paths, list)
        is_merge_commit = title.startswith("Merge ")
        is_last_sync_merge = title.startswith("Merge branch 'sync/")
        if is_merge_commit and (is_last_sync_merge or not paths):
            continue
        filtered.append(commit)
    return filtered
