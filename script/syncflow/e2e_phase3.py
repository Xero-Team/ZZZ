from __future__ import annotations

import json
import os
import shutil
import subprocess
from pathlib import Path


def run(command: list[str], *, cwd: Path, env: dict[str, str] | None = None) -> str:
    result = subprocess.run(
        command,
        cwd=cwd,
        check=True,
        text=True,
        capture_output=True,
        env=env,
    )
    return result.stdout.strip()


def run_json(command: list[str], *, cwd: Path, env: dict[str, str] | None = None) -> dict[str, object]:
    return json.loads(run(command, cwd=cwd, env=env))


def expect_failure(
    command: list[str],
    *,
    cwd: Path,
    env: dict[str, str] | None = None,
    expected_error_code: str,
) -> dict[str, object]:
    result = subprocess.run(
        command,
        cwd=cwd,
        check=False,
        text=True,
        capture_output=True,
        env=env,
    )
    if result.returncode == 0:
        raise AssertionError(f"Command unexpectedly succeeded: {' '.join(command)}")
    payload = json.loads(result.stdout.strip())
    if payload.get("error_code") != expected_error_code:
        raise AssertionError(payload)
    return payload


def cli(cli_root: Path, env: dict[str, str], *args: str) -> dict[str, object]:
    payload = run_json(["uv", "run", "python", "-m", "syncflow", *args], cwd=cli_root, env=env)
    if payload.get("ok") is not True:
        raise AssertionError(payload)
    return payload["data"]  # type: ignore[return-value]


def main() -> None:
    source_root = Path(__file__).resolve().parents[2]
    cli_root = source_root / "script" / "syncflow"
    temp_root = Path("/tmp/opencode/zzz-syncflow-phase3-e2e")
    if temp_root.exists():
        shutil.rmtree(temp_root)

    run(["git", "clone", "--quiet", str(source_root), str(temp_root)], cwd=source_root)
    base_sha = run(["git", "rev-parse", "HEAD"], cwd=temp_root)
    run(["git", "checkout", "-b", "sync/upstream-phase3-e2e"], cwd=temp_root)

    docs_path = temp_root / "docs" / "PHASE3_E2E.md"
    docs_path.write_text("phase3 docs route\n", encoding="utf-8")
    run(["git", "add", str(docs_path.relative_to(temp_root))], cwd=temp_root)
    run(["git", "commit", "-m", "phase3 docs route"], cwd=temp_root)

    workflow_path = temp_root / ".github" / "workflows" / "phase3-e2e.yml"
    workflow_path.write_text("name: phase3-e2e\non: workflow_dispatch\n", encoding="utf-8")
    run(["git", "add", str(workflow_path.relative_to(temp_root))], cwd=temp_root)
    run(["git", "commit", "-m", "phase3 workflow route"], cwd=temp_root)

    rust_path = temp_root / "crates" / "zed" / "src" / "phase3_e2e.rs"
    rust_path.write_text("pub fn phase3_e2e() {}\n", encoding="utf-8")
    run(["git", "add", str(rust_path.relative_to(temp_root))], cwd=temp_root)
    run(["git", "commit", "-m", "phase3 rust route"], cwd=temp_root)

    privacy_path = temp_root / "docs" / "privacy-phase3.md"
    privacy_path.write_text("privacy policy note\n", encoding="utf-8")
    run(["git", "add", str(privacy_path.relative_to(temp_root))], cwd=temp_root)
    run(["git", "commit", "-m", "phase3 privacy route"], cwd=temp_root)

    upstream_sha = run(["git", "rev-parse", "HEAD"], cwd=temp_root)
    run(["git", "checkout", "main"], cwd=temp_root)

    env = {**os.environ, "SYNCFLOW_REPO_ROOT": str(temp_root)}
    run_id = "2026-05-24T00-00-00Z"

    cli(
        cli_root,
        env,
        "init-run",
        "--base-branch",
        "main",
        "--sync-branch",
        "sync/upstream-phase3-e2e",
        "--upstream-head",
        upstream_sha,
        "--merge-base",
        base_sha,
        "--run-id",
        run_id,
    )
    cli(cli_root, env, "intake-candidates", "--run-id", run_id)

    commit_rows = cli(cli_root, env, "list-commits", "--run-id", run_id)["commits"]
    title_to_commit = {item["title"]: item["commit_id"] for item in commit_rows}  # type: ignore[index]

    for title in [
        "phase3 docs route",
        "phase3 workflow route",
        "phase3 rust route",
        "phase3 privacy route",
    ]:
        cli(
            cli_root,
            env,
            "score-commit",
            "--run-id",
            run_id,
            "--commit-id",
            title_to_commit[title],
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

    candidates = cli(cli_root, env, "auto-resolve-candidates", "--run-id", run_id)
    eligible_commit_ids = set(candidates["eligible_commit_ids"])
    docs_commit_id = title_to_commit["phase3 docs route"]
    workflow_commit_id = title_to_commit["phase3 workflow route"]
    rust_commit_id = title_to_commit["phase3 rust route"]
    privacy_commit_id = title_to_commit["phase3 privacy route"]

    assert docs_commit_id in eligible_commit_ids
    assert workflow_commit_id in eligible_commit_ids
    assert rust_commit_id not in eligible_commit_ids
    assert privacy_commit_id not in eligible_commit_ids

    cli(
        cli_root,
        env,
        "auto-resolve-commit",
        "--run-id",
        run_id,
        "--commit-id",
        docs_commit_id,
        "--agent",
        "docs-policy-agent",
        "--summary",
        "phase3 docs auto route",
    )
    cli(
        cli_root,
        env,
        "auto-resolve-commit",
        "--run-id",
        run_id,
        "--commit-id",
        workflow_commit_id,
        "--agent",
        "workflow-merge-agent",
        "--summary",
        "phase3 workflow auto route",
    )

    expect_failure(
        [
            "uv",
            "run",
            "python",
            "-m",
            "syncflow",
            "auto-resolve-commit",
            "--run-id",
            run_id,
            "--commit-id",
            rust_commit_id,
            "--agent",
            "rust-merge-agent",
            "--summary",
            "should fail",
        ],
        cwd=cli_root,
        env=env,
        expected_error_code="auto_resolve_not_allowed",
    )

    rust_report = cli(
        cli_root,
        env,
        "create-report",
        "--run-id",
        run_id,
        "--commit-id",
        rust_commit_id,
        "--title",
        "Phase3 rust report",
    )["report_id"]
    rust_decision = cli(
        cli_root,
        env,
        "record-decision",
        "--run-id",
        run_id,
        "--target-type",
        "report",
        "--target-id",
        rust_report,
        "--chosen-action",
        "manual-rework",
        "--rationale-summary",
        "rust path needs human review",
        "--human-feedback",
        "challenger required",
    )["decision_id"]

    expect_failure(
        [
            "uv",
            "run",
            "python",
            "-m",
            "syncflow",
            "finalize-run",
            "--run-id",
            run_id,
            "--outcome",
            "completed",
            "--no-merge-to-base",
            "--keep-local-branch",
            "--keep-remote-branch",
        ],
        cwd=cli_root,
        env=env,
        expected_error_code="unresolved_decisions_present",
    )

    rust_debate = cli(
        cli_root,
        env,
        "record-debate",
        "--run-id",
        run_id,
        "--target-id",
        rust_decision,
        "--trigger-reason",
        "policy-sensitive",
        "--recommendation",
        "manual-rework",
        "--disagreement-level",
        "medium",
    )["debate_id"]
    cli(
        cli_root,
        env,
        "consensus-review",
        "--run-id",
        run_id,
        "--decision-id",
        rust_decision,
        "--debate-id",
        rust_debate,
        "--disagreement-level",
        "mostly-resolved",
        "--summary",
        "rust review completed",
        "--no-escalate-to-human",
    )
    cli(
        cli_root,
        env,
        "apply-decision",
        "--run-id",
        run_id,
        "--decision-id",
        rust_decision,
        "--applied-by",
        "decision-apply-agent",
        "--applied-summary",
        "rust path handled manually",
        "--applied-path",
        "crates/zed/src/phase3_e2e.rs",
        "--git-commit-sha",
        upstream_sha,
    )

    privacy_report = cli(
        cli_root,
        env,
        "create-report",
        "--run-id",
        run_id,
        "--commit-id",
        privacy_commit_id,
        "--title",
        "Phase3 privacy report",
    )["report_id"]
    privacy_decision = cli(
        cli_root,
        env,
        "record-decision",
        "--run-id",
        run_id,
        "--target-type",
        "report",
        "--target-id",
        privacy_report,
        "--chosen-action",
        "keep-zzz",
        "--rationale-summary",
        "privacy path escalated",
        "--human-feedback",
        "challenger required",
    )["decision_id"]
    privacy_debate = cli(
        cli_root,
        env,
        "record-debate",
        "--run-id",
        run_id,
        "--target-id",
        privacy_decision,
        "--trigger-reason",
        "policy-sensitive",
        "--recommendation",
        "keep-zzz",
        "--disagreement-level",
        "medium",
    )["debate_id"]
    cli(
        cli_root,
        env,
        "consensus-review",
        "--run-id",
        run_id,
        "--decision-id",
        privacy_decision,
        "--debate-id",
        privacy_debate,
        "--disagreement-level",
        "mostly-resolved",
        "--summary",
        "privacy review completed",
        "--no-escalate-to-human",
    )
    cli(
        cli_root,
        env,
        "apply-decision",
        "--run-id",
        run_id,
        "--decision-id",
        privacy_decision,
        "--applied-by",
        "decision-apply-agent",
        "--applied-summary",
        "privacy path kept manual",
        "--applied-path",
        "docs/privacy-phase3.md",
        "--git-commit-sha",
        upstream_sha,
    )

    list_commits = cli(cli_root, env, "list-commits", "--run-id", run_id)
    list_decisions = cli(cli_root, env, "list-decisions", "--run-id", run_id, "--resolution-status", "resolved")
    list_debates = cli(cli_root, env, "list-debates", "--run-id", run_id, "--status", "completed")

    assert len(list_commits["commits"]) == 4
    assert len(list_decisions["decisions"]) == 2
    assert len(list_debates["debates"]) == 2

    cli(
        cli_root,
        env,
        "finalize-run",
        "--run-id",
        run_id,
        "--outcome",
        "completed",
        "--no-merge-to-base",
        "--keep-local-branch",
        "--keep-remote-branch",
    )
    consistency = cli(cli_root, env, "check-run-consistency", "--run-id", run_id)
    history = cli(cli_root, env, "get-history-summary")
    assert consistency["ok"] is True
    assert history["run_count"] >= 1
    print(json.dumps({"consistency": consistency, "history": history}, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
