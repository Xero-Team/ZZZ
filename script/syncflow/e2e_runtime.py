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


def expect_failure(
    cli_root: Path,
    env: dict[str, str],
    expected_error_code: str,
    *args: str,
) -> dict[str, object]:
    result = subprocess.run(
        ["uv", "run", "python", "-m", "syncflow", *args],
        cwd=cli_root,
        check=False,
        text=True,
        capture_output=True,
        env=env,
    )
    if result.returncode == 0:
        raise AssertionError(f"Command unexpectedly succeeded: {' '.join(args)}")
    payload = json.loads(result.stdout.strip())
    if payload.get("error_code") != expected_error_code:
        raise AssertionError(payload)
    return payload


def main() -> None:
    source_root = Path(__file__).resolve().parents[2]
    cli_root = source_root / "script" / "syncflow"
    temp_root = Path("/tmp/opencode/zzz-syncflow-runtime-e2e")
    if temp_root.exists():
        shutil.rmtree(temp_root)

    run(["git", "clone", "--quiet", str(source_root), str(temp_root)], cwd=source_root)
    run(["git", "remote", "add", "upstream", str(source_root)], cwd=temp_root)

    env = {**os.environ, "SYNCFLOW_REPO_ROOT": str(temp_root)}
    run_id = "2026-05-24T01-00-00Z"

    preflight = cli(cli_root, env, "preflight")
    sync_branch = str(preflight["sync_branch"])
    assert sync_branch.startswith("sync/upstream-")
    assert preflight["upstream_ref"] == "upstream/main"

    started = cli(cli_root, env, "start-run", "--run-id", run_id)
    assert started["run_id"] == run_id
    assert started["created_sync_branch"] == sync_branch
    assert run(["git", "rev-parse", "--abbrev-ref", "HEAD"], cwd=temp_root) == sync_branch

    active = cli(cli_root, env, "get-active-run")
    assert active["active_run_id"] == run_id
    assert active["session"]["phase"] == "init-run"
    assert active["session"]["upstream_remote"] == "upstream"

    expect_failure(cli_root, env, "active_run_exists", "preflight")

    finalized = cli(
        cli_root,
        env,
        "finalize-run",
        "--run-id",
        run_id,
        "--outcome",
        "completed",
        "--no-merge-to-base",
        "--delete-local-branch",
        "--keep-remote-branch",
    )
    assert finalized["sync_branch_deleted_local"] is True
    assert finalized["sync_branch_deleted_remote"] is False
    assert run(["git", "rev-parse", "--abbrev-ref", "HEAD"], cwd=temp_root) == "main"

    cleanup = cli(cli_root, env, "cleanup-sync-branch", "--run-id", run_id)
    assert cleanup["deleted_local"] is False
    assert cleanup["deleted_remote"] is False
    history = cli(cli_root, env, "get-history-summary")
    assert history["run_count"] >= 1
    print(
        json.dumps(
            {
                "preflight": preflight,
                "finalized": finalized,
                "cleanup": cleanup,
                "history": history,
            },
            indent=2,
            ensure_ascii=False,
        )
    )


if __name__ == "__main__":
    main()
