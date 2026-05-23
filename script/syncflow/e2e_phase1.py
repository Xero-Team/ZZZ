from __future__ import annotations

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


def main() -> None:
    source_root = Path(__file__).resolve().parents[2]
    cli_root = source_root / "script" / "syncflow"
    temp_root = Path("/tmp/opencode/zzz-syncflow-phase1-e2e")
    if temp_root.exists():
        shutil.rmtree(temp_root)

    run(["git", "clone", "--quiet", str(source_root), str(temp_root)], cwd=source_root)
    base_sha = run(["git", "rev-parse", "HEAD"], cwd=temp_root)
    run(["git", "checkout", "-b", "sync/upstream-phase1-e2e"], cwd=temp_root)
    (temp_root / "PHASE1_E2E.txt").write_text("phase1 e2e\n", encoding="utf-8")
    run(["git", "add", "PHASE1_E2E.txt"], cwd=temp_root)
    run(["git", "commit", "-m", "phase1 e2e change"], cwd=temp_root)
    upstream_sha = run(["git", "rev-parse", "HEAD"], cwd=temp_root)
    run(["git", "checkout", "main"], cwd=temp_root)

    env = {**os.environ, "SYNCFLOW_REPO_ROOT": str(temp_root)}
    run(
        [
            "uv",
            "run",
            "python",
            "-m",
            "syncflow",
            "init-run",
            "--base-branch",
            "main",
            "--sync-branch",
            "sync/upstream-phase1-e2e",
            "--upstream-head",
            upstream_sha,
            "--merge-base",
            base_sha,
            "--run-id",
            "2026-05-23T23-59-59Z",
        ],
        cwd=cli_root,
        env=env,
    )
    run(["uv", "run", "python", "-m", "syncflow", "intake-candidates", "--run-id", "2026-05-23T23-59-59Z"], cwd=cli_root, env=env)
    run([
        "uv", "run", "python", "-m", "syncflow", "score-commit",
        "--run-id", "2026-05-23T23-59-59Z",
        "--commit-id", "CMP-0001",
        "--agent", "commit-score-reviewer",
        "--philosophy-fit", "3",
        "--merge-risk", "1",
        "--behavior-regression-risk", "1",
        "--maintenance-cost", "1",
        "--upstream-alignment", "4",
        "--confidence", "3",
    ], cwd=cli_root, env=env)
    run([
        "uv", "run", "python", "-m", "syncflow", "create-report",
        "--run-id", "2026-05-23T23-59-59Z",
        "--commit-id", "CMP-0001",
        "--title", "Phase1 E2E report",
    ], cwd=cli_root, env=env)
    run([
        "uv", "run", "python", "-m", "syncflow", "record-decision",
        "--run-id", "2026-05-23T23-59-59Z",
        "--target-type", "report",
        "--target-id", "RPT-0001",
        "--chosen-action", "accept-upstream",
        "--rationale-summary", "Phase1 e2e decision",
        "--human-feedback", "Challenger review required",
    ], cwd=cli_root, env=env)
    run([
        "uv", "run", "python", "-m", "syncflow", "record-debate",
        "--run-id", "2026-05-23T23-59-59Z",
        "--target-id", "DEC-0001",
        "--trigger-reason", "human-decision",
        "--recommendation", "accept-upstream",
        "--disagreement-level", "medium",
    ], cwd=cli_root, env=env)
    run([
        "uv", "run", "python", "-m", "syncflow", "consensus-review",
        "--run-id", "2026-05-23T23-59-59Z",
        "--decision-id", "DEC-0001",
        "--debate-id", "DEB-0001",
        "--disagreement-level", "mostly-resolved",
        "--summary", "Phase1 e2e consensus",
        "--no-escalate-to-human",
    ], cwd=cli_root, env=env)
    run([
        "uv", "run", "python", "-m", "syncflow", "apply-decision",
        "--run-id", "2026-05-23T23-59-59Z",
        "--decision-id", "DEC-0001",
        "--applied-by", "decision-apply-agent",
        "--applied-summary", "Applied e2e sample change",
        "--applied-path", "PHASE1_E2E.txt",
        "--git-commit-sha", upstream_sha,
    ], cwd=cli_root, env=env)
    run([
        "uv", "run", "python", "-m", "syncflow", "finalize-run",
        "--run-id", "2026-05-23T23-59-59Z",
        "--outcome", "completed",
        "--merge-to-base",
        "--delete-local-branch",
        "--keep-remote-branch",
    ], cwd=cli_root, env=env)
    output = run([
        "uv", "run", "python", "-m", "syncflow", "check-run-consistency",
        "--run-id", "2026-05-23T23-59-59Z",
    ], cwd=cli_root, env=env)
    print(output)


if __name__ == "__main__":
    main()
