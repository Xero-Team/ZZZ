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


def cli(cli_root: Path, env: dict[str, str], *args: str) -> dict[str, object]:
    payload = run_json(["uv", "run", "python", "-m", "syncflow", *args], cwd=cli_root, env=env)
    if payload.get("ok") is not True:
        raise AssertionError(payload)
    return payload["data"]  # type: ignore[return-value]


def main() -> None:
    source_root = Path(__file__).resolve().parents[2]
    cli_root = source_root / "script" / "syncflow"
    temp_root = Path("/tmp/opencode/zzz-syncflow-phase2-e2e")
    if temp_root.exists():
        shutil.rmtree(temp_root)

    run(["git", "clone", "--quiet", str(source_root), str(temp_root)], cwd=source_root)
    run(["git", "checkout", "-b", "sync/upstream-phase2-e2e"], cwd=temp_root)
    docs_path = temp_root / "docs" / "PHASE2_E2E.md"
    docs_path.write_text("phase2 docs diff\n", encoding="utf-8")
    run(["git", "add", str(docs_path.relative_to(temp_root))], cwd=temp_root)
    run(["git", "commit", "-m", "phase2 docs diff"], cwd=temp_root)
    upstream_sha = run(["git", "rev-parse", "HEAD"], cwd=temp_root)
    base_sha = run(["git", "rev-parse", "HEAD~1"], cwd=temp_root)
    run(["git", "checkout", "main"], cwd=temp_root)

    env = {**os.environ, "SYNCFLOW_REPO_ROOT": str(temp_root)}
    run_id = "2026-05-24T02-00-00Z"

    cli(
        cli_root,
        env,
        "init-run",
        "--base-branch",
        "main",
        "--sync-branch",
        "sync/upstream-phase2-e2e",
        "--upstream-head",
        upstream_sha,
        "--merge-base",
        base_sha,
        "--run-id",
        run_id,
    )
    cli(cli_root, env, "intake-candidates", "--run-id", run_id)
    cli(cli_root, env, "assign-domain", "--run-id", run_id, "--commit-id", "CMP-0001", "--domain", "docs")
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

    report_id = str(
        cli(
            cli_root,
            env,
            "create-report",
            "--run-id",
            run_id,
            "--commit-id",
            "CMP-0001",
            "--title",
            "Phase2 report",
        )["report_id"]
    )
    cli(
        cli_root,
        env,
        "update-report",
        "--run-id",
        run_id,
        "--report-id",
        report_id,
        "--body",
        "# Phase2 report\n\nUpdated report body.\n",
    )
    report = cli(cli_root, env, "get-report-by-id", "--run-id", run_id, "--report-id", report_id)
    assert "Updated report body." in str(report["body"])
    listed_active = cli(cli_root, env, "list-reports", "--run-id", run_id, "--status", "active")
    assert len(listed_active["reports"]) == 1
    cli(
        cli_root,
        env,
        "write-agent-result",
        "--run-id",
        run_id,
        "--agent",
        "report-drafter-agent",
        "--target-type",
        "report",
        "--target-id",
        report_id,
        "--status",
        "drafted",
        "--summary",
        "phase2 report drafted",
        "--payload-json",
        '{"style":"concise","confidence":"high"}',
    )

    decision_id = str(
        cli(
            cli_root,
            env,
            "record-decision",
            "--run-id",
            run_id,
            "--target-type",
            "report",
            "--target-id",
            report_id,
            "--chosen-action",
            "accept-upstream",
            "--rationale-summary",
            "phase2 review complete",
            "--human-feedback",
            "challenger required",
        )["decision_id"]
    )
    debate_id = str(
        cli(
            cli_root,
            env,
            "record-debate",
            "--run-id",
            run_id,
            "--target-id",
            decision_id,
            "--trigger-reason",
            "human-decision",
            "--recommendation",
            "accept-upstream",
            "--disagreement-level",
            "medium",
        )["debate_id"]
    )
    cli(
        cli_root,
        env,
        "update-report-index",
        "--run-id",
        run_id,
        "--report-id",
        report_id,
        "--decision-id",
        decision_id,
        "--debate-id",
        debate_id,
        "--status",
        "active",
    )
    cli(
        cli_root,
        env,
        "complete-debate",
        "--run-id",
        run_id,
        "--debate-id",
        debate_id,
        "--status",
        "completed",
        "--disagreement-level",
        "mostly-resolved",
    )
    cli(
        cli_root,
        env,
        "consensus-review",
        "--run-id",
        run_id,
        "--decision-id",
        decision_id,
        "--debate-id",
        debate_id,
        "--disagreement-level",
        "mostly-resolved",
        "--summary",
        "phase2 consensus",
        "--no-escalate-to-human",
    )
    cli(
        cli_root,
        env,
        "apply-decision",
        "--run-id",
        run_id,
        "--decision-id",
        decision_id,
        "--applied-by",
        "decision-apply-agent",
        "--applied-summary",
        "phase2 apply",
        "--applied-path",
        "docs/PHASE2_E2E.md",
        "--git-commit-sha",
        upstream_sha,
    )
    cli(cli_root, env, "archive-report", "--run-id", run_id, "--report-id", report_id)

    listed_archived = cli(cli_root, env, "list-reports", "--run-id", run_id, "--status", "archived")
    assert len(listed_archived["reports"]) == 1

    summary = cli(cli_root, env, "get-run-summary", "--run-id", run_id)
    assert summary["counts"]["reports_active"] == 0
    assert summary["counts"]["reports_archived"] == 1

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
    assert consistency["ok"] is True
    print(json.dumps({"summary": summary, "consistency": consistency}, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
