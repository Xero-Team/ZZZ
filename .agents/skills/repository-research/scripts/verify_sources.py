#!/usr/bin/env python3
"""Verify repository URLs from SOURCES.tsv with non-interactive git ls-remote."""

from __future__ import annotations

import argparse
import re
import subprocess
from pathlib import Path

from _common import (
    has_embedded_credentials,
    read_tsv,
    redact_url,
    run_command,
    truncate,
    utc_now,
    write_tsv,
)

FIELDS = [
    "id",
    "requested_name",
    "requested_url",
    "resolved_url",
    "clone_path",
    "status",
    "notes",
    "checked_at",
    "verification_status",
    "remote_head_ref",
    "remote_head_sha",
    "verified_at",
    "verification_error",
]


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sources", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--timeout", type=int, default=30)
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Exit nonzero when a direct/substitute source cannot be verified.",
    )
    return parser.parse_args()


def parse_remote_head(output: str) -> tuple[str, str]:
    head_ref = ""
    head_sha = ""
    for line in output.splitlines():
        if line.startswith("ref: ") and line.endswith("\tHEAD"):
            head_ref = line.removeprefix("ref: ").removesuffix("\tHEAD")
        else:
            match = re.fullmatch(r"([0-9a-fA-F]{40,64})\tHEAD", line)
            if match:
                head_sha = match.group(1).lower()
    return head_ref, head_sha


def main() -> int:
    arguments = parse_arguments()
    rows = read_tsv(arguments.sources)
    failures = 0

    for row in rows:
        row["requested_url"] = redact_url(row.get("requested_url", ""))
        row["verified_at"] = utc_now()
        row["verification_error"] = ""
        row["remote_head_ref"] = ""
        row["remote_head_sha"] = ""
        status = row.get("status", "").strip()
        url = row.get("resolved_url", "").strip()
        row["resolved_url"] = redact_url(url)

        if status == "unavailable" or not url:
            row["verification_status"] = "skipped"
            if status != "unavailable":
                row["verification_error"] = "resolved_url is empty"
                failures += 1
            continue

        if has_embedded_credentials(url):
            row["verification_status"] = "rejected"
            row["verification_error"] = "credential-bearing remote URLs are not allowed"
            failures += 1
            continue

        try:
            result = run_command(
                ["git", "ls-remote", "--symref", url, "HEAD"],
                timeout=arguments.timeout,
            )
        except subprocess.TimeoutExpired:
            row["verification_status"] = "timeout"
            row["verification_error"] = f"git ls-remote exceeded {arguments.timeout}s"
            failures += 1
            continue
        except Exception as error:
            row["verification_status"] = "error"
            row["verification_error"] = truncate(str(error))
            failures += 1
            continue

        if result.returncode != 0:
            row["verification_status"] = "unreachable"
            row["verification_error"] = truncate((result.stderr or result.stdout).replace(url, redact_url(url)))
            failures += 1
            continue

        head_ref, head_sha = parse_remote_head(result.stdout)
        row["verification_status"] = "verified"
        row["remote_head_ref"] = head_ref
        row["remote_head_sha"] = head_sha

    write_tsv(arguments.output, rows, FIELDS)
    verified = sum(row.get("verification_status") == "verified" for row in rows)
    print(f"verified={verified} failures={failures} output={arguments.output}")
    return 1 if arguments.strict and failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
