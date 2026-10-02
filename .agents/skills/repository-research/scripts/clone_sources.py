#!/usr/bin/env python3
"""Safely shallow-clone repositories listed in a source ledger."""

from __future__ import annotations

import argparse
import concurrent.futures
import subprocess
from pathlib import Path

from _common import (
    git,
    has_embedded_credentials,
    is_git_repository,
    read_tsv,
    redact_url,
    resolve_under_base,
    run_command,
    truncate,
    utc_now,
    write_tsv,
)

FIELDS = [
    "id",
    "requested_name",
    "resolved_url",
    "clone_path",
    "source_status",
    "clone_status",
    "head_sha",
    "origin_url",
    "completed_at",
    "message",
]


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sources", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--base-dir", type=Path, default=Path.cwd())
    parser.add_argument("--depth", type=int, default=1)
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--timeout", type=int, default=600)
    parser.add_argument("--dry-run", action="store_true")
    return parser.parse_args()


def inspect_or_clone(row: dict[str, str], arguments: argparse.Namespace) -> dict[str, str]:
    result = {
        "id": row.get("id", ""),
        "requested_name": row.get("requested_name", ""),
        "resolved_url": redact_url(row.get("resolved_url", "")),
        "clone_path": row.get("clone_path", ""),
        "source_status": row.get("status", ""),
        "clone_status": "",
        "head_sha": "",
        "origin_url": "",
        "completed_at": utc_now(),
        "message": "",
    }
    if result["source_status"] == "unavailable":
        result["clone_status"] = "skipped"
        result["message"] = "source marked unavailable"
        return result
    raw_url = row.get("resolved_url", "")
    if has_embedded_credentials(raw_url):
        result["clone_status"] = "invalid"
        result["message"] = "credential-bearing remote URLs are not allowed"
        return result
    if row.get("verification_status") and row["verification_status"] != "verified":
        result["clone_status"] = "skipped"
        result["message"] = f"verification_status={row['verification_status']}"
        return result
    if not result["resolved_url"] or not result["clone_path"]:
        result["clone_status"] = "invalid"
        result["message"] = "resolved_url and clone_path are required"
        return result

    try:
        destination = resolve_under_base(result["clone_path"], arguments.base_dir)
    except ValueError as error:
        result["clone_status"] = "invalid"
        result["message"] = str(error)
        return result

    if destination.exists():
        if not is_git_repository(destination):
            result["clone_status"] = "blocked"
            result["message"] = "destination exists and is not a Git repository"
            return result
        try:
            result["head_sha"] = git(destination, "rev-parse", "HEAD")
            result["origin_url"] = redact_url(git(destination, "remote", "get-url", "origin"))
        except RuntimeError as error:
            result["clone_status"] = "error"
            result["message"] = truncate(str(error))
            return result
        result["clone_status"] = "existing"
        if result["origin_url"] != result["resolved_url"]:
            result["message"] = "existing origin differs from resolved_url"
        return result

    if arguments.dry_run:
        result["clone_status"] = "planned"
        return result

    destination.parent.mkdir(parents=True, exist_ok=True)
    command = [
        "git",
        "clone",
        "--depth",
        str(arguments.depth),
        "--single-branch",
        "--no-tags",
        "--filter=blob:none",
        raw_url,
        str(destination),
    ]
    try:
        completed = run_command(command, timeout=arguments.timeout)
    except subprocess.TimeoutExpired:
        result["clone_status"] = "error"
        result["message"] = f"git clone exceeded {arguments.timeout}s"
        return result
    except Exception as error:
        result["clone_status"] = "error"
        result["message"] = truncate(str(error))
        return result
    if completed.returncode != 0:
        result["clone_status"] = "failed"
        result["message"] = truncate((completed.stderr or completed.stdout).replace(raw_url, result["resolved_url"]))
        return result

    result["clone_status"] = "cloned"
    result["head_sha"] = git(destination, "rev-parse", "HEAD")
    result["origin_url"] = redact_url(git(destination, "remote", "get-url", "origin"))
    return result


def main() -> int:
    arguments = parse_arguments()
    arguments.base_dir = arguments.base_dir.resolve()
    rows = read_tsv(arguments.sources)
    with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, arguments.jobs)) as executor:
        results = list(executor.map(lambda row: inspect_or_clone(row, arguments), rows))
    results.sort(key=lambda row: row["id"])
    write_tsv(arguments.output, results, FIELDS)
    failures = sum(row["clone_status"] in {"invalid", "blocked", "error", "failed"} for row in results)
    print(f"rows={len(results)} failures={failures} output={arguments.output}")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
