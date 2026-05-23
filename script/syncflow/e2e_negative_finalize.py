from __future__ import annotations

import json
import os
import shutil
import subprocess
from pathlib import Path


def run(command: list[str], *, cwd: Path, env: dict[str, str] | None = None, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=cwd,
        check=check,
        text=True,
        capture_output=True,
        env=env,
    )


def cli(cli_root: Path, env: dict[str, str], *args: str) -> dict[str, object]:
    result = run(["uv", "run", "python", "-m", "syncflow", *args], cwd=cli_root, env=env)
    payload = json.loads(result.stdout.strip())
    if payload.get("ok") is not True:
        raise AssertionError(payload)
    return payload["data"]  # type: ignore[return-value]


def expect_failure(cli_root: Path, env: dict[str, str], expected_error_code: str, *args: str) -> None:
    result = run(["uv", "run", "python", "-m", "syncflow", *args], cwd=cli_root, env=env, check=False)
    if result.returncode == 0:
        raise AssertionError(f"Command unexpectedly succeeded: {' '.join(args)}")
    payload = json.loads(result.stdout.strip())
    if payload.get("error_code") != expected_error_code:
        raise AssertionError(payload)


def init_basic_run(cli_root: Path, env: dict[str, str], run_id: str, sync_branch: str, upstream_sha: str, base_sha: str) -> None:
    cli(
        cli_root,
        env,
        "init-run",
        "--base-branch",
        "main",
        "--sync-branch",
        sync_branch,
        "--upstream-head",
        upstream_sha,
        "--merge-base",
        base_sha,
        "--run-id",
        run_id,
    )
    cli(cli_root, env, "intake-candidates", "--run-id", run_id)
    cli(
        cli_root,
        env,
        "score-commit",
        "--run-id",
        run_id,
        "--commit-id",
        "CMP-0001",
        "--agent",
        "commit-score-reviewer",
        "--philosophy-fit",
        "4",
        "--merge-risk",
        "1",
        "--behavior-regression-risk",
        "1",
        "--maintenance-cost",
        "1",
        "--upstream-alignment",
        "4",
        "--confidence",
        "4",
    )


def make_repo(source_root: Path, temp_root: Path) -> tuple[dict[str, str], str, str]:
    if temp_root.exists():
        shutil.rmtree(temp_root)
    run(["git", "clone", "--quiet", str(source_root), str(temp_root)], cwd=source_root)
    run(["git", "checkout", "-b", "sync/upstream-negative-e2e"], cwd=temp_root)
    path = temp_root / "docs" / "NEGATIVE_E2E.md"
    path.write_text("negative e2e\n", encoding="utf-8")
    run(["git", "add", str(path.relative_to(temp_root))], cwd=temp_root)
    run(["git", "commit", "-m", "negative finalize fixture"], cwd=temp_root)
    upstream_sha = run(["git", "rev-parse", "HEAD"], cwd=temp_root).stdout.strip()
    base_sha = run(["git", "rev-parse", "HEAD~1"], cwd=temp_root).stdout.strip()
    run(["git", "checkout", "main"], cwd=temp_root)
    env = {**os.environ, "SYNCFLOW_REPO_ROOT": str(temp_root)}
    return env, upstream_sha, base_sha


def main() -> None:
    source_root = Path(__file__).resolve().parents[2]
    cli_root = source_root / "script" / "syncflow"

    env, upstream_sha, base_sha = make_repo(source_root, Path("/tmp/opencode/zzz-syncflow-negative-unresolved"))
    init_basic_run(cli_root, env, "2026-05-24T03-00-00Z", "sync/upstream-negative-unresolved", upstream_sha, base_sha)
    report_id = str(cli(cli_root, env, "create-report", "--run-id", "2026-05-24T03-00-00Z", "--commit-id", "CMP-0001", "--title", "Negative unresolved")["report_id"])
    cli(cli_root, env, "record-decision", "--run-id", "2026-05-24T03-00-00Z", "--target-type", "report", "--target-id", report_id, "--chosen-action", "accept-upstream", "--rationale-summary", "pending", "--human-feedback", "pending")
    expect_failure(cli_root, env, "unresolved_decisions_present", "finalize-run", "--run-id", "2026-05-24T03-00-00Z", "--outcome", "completed", "--no-merge-to-base", "--keep-local-branch", "--keep-remote-branch")

    env, upstream_sha, base_sha = make_repo(source_root, Path("/tmp/opencode/zzz-syncflow-negative-blocking"))
    init_basic_run(cli_root, env, "2026-05-24T03-10-00Z", "sync/upstream-negative-blocking", upstream_sha, base_sha)
    cli(cli_root, env, "score-commit", "--run-id", "2026-05-24T03-10-00Z", "--commit-id", "CMP-0001", "--agent", "commit-score-reviewer", "--philosophy-fit", "1", "--merge-risk", "4", "--behavior-regression-risk", "4", "--maintenance-cost", "2", "--upstream-alignment", "1", "--confidence", "2")
    expect_failure(cli_root, env, "blocking_items_present", "finalize-run", "--run-id", "2026-05-24T03-10-00Z", "--outcome", "completed", "--no-merge-to-base", "--keep-local-branch", "--keep-remote-branch")

    env, upstream_sha, base_sha = make_repo(source_root, Path("/tmp/opencode/zzz-syncflow-negative-open-debate"))
    init_basic_run(cli_root, env, "2026-05-24T03-20-00Z", "sync/upstream-negative-open-debate", upstream_sha, base_sha)
    report_id = str(cli(cli_root, env, "create-report", "--run-id", "2026-05-24T03-20-00Z", "--commit-id", "CMP-0001", "--title", "Negative debate")["report_id"])
    decision_id = str(cli(cli_root, env, "record-decision", "--run-id", "2026-05-24T03-20-00Z", "--target-type", "report", "--target-id", report_id, "--chosen-action", "accept-upstream", "--rationale-summary", "pending", "--human-feedback", "pending")["decision_id"])
    cli(cli_root, env, "record-debate", "--run-id", "2026-05-24T03-20-00Z", "--target-id", decision_id, "--trigger-reason", "human-decision", "--recommendation", "accept-upstream", "--disagreement-level", "medium")
    cli(cli_root, env, "mark-resolved", "--run-id", "2026-05-24T03-20-00Z", "--decision-id", decision_id, "--resolution-status", "resolved")
    expect_failure(cli_root, env, "challenger_review_incomplete", "finalize-run", "--run-id", "2026-05-24T03-20-00Z", "--outcome", "completed", "--no-merge-to-base", "--keep-local-branch", "--keep-remote-branch")

    env, upstream_sha, base_sha = make_repo(source_root, Path("/tmp/opencode/zzz-syncflow-negative-high-disagreement"))
    init_basic_run(cli_root, env, "2026-05-24T03-30-00Z", "sync/upstream-negative-high-disagreement", upstream_sha, base_sha)
    report_id = str(cli(cli_root, env, "create-report", "--run-id", "2026-05-24T03-30-00Z", "--commit-id", "CMP-0001", "--title", "Negative consensus")["report_id"])
    decision_id = str(cli(cli_root, env, "record-decision", "--run-id", "2026-05-24T03-30-00Z", "--target-type", "report", "--target-id", report_id, "--chosen-action", "accept-upstream", "--rationale-summary", "pending", "--human-feedback", "pending")["decision_id"])
    debate_id = str(cli(cli_root, env, "record-debate", "--run-id", "2026-05-24T03-30-00Z", "--target-id", decision_id, "--trigger-reason", "human-decision", "--recommendation", "accept-upstream", "--disagreement-level", "high-disagreement")["debate_id"])
    cli(cli_root, env, "consensus-review", "--run-id", "2026-05-24T03-30-00Z", "--decision-id", decision_id, "--debate-id", debate_id, "--disagreement-level", "high-disagreement", "--summary", "still disputed", "--no-escalate-to-human")
    cli(cli_root, env, "mark-resolved", "--run-id", "2026-05-24T03-30-00Z", "--decision-id", decision_id, "--resolution-status", "resolved")
    expect_failure(cli_root, env, "high_disagreement_present", "finalize-run", "--run-id", "2026-05-24T03-30-00Z", "--outcome", "completed", "--no-merge-to-base", "--keep-local-branch", "--keep-remote-branch")

    print(json.dumps({"ok": True, "checked": 4}, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
