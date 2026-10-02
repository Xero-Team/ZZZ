#!/usr/bin/env python3
"""Shared helpers for repository-research scripts."""

from __future__ import annotations

import csv
import datetime as dt
import os
import re
import subprocess
import tempfile
from pathlib import Path
from typing import Iterable, Sequence
from urllib.parse import urlsplit, urlunsplit


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def read_tsv(path: Path) -> list[dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle, delimiter="\t"))


def write_tsv(path: Path, rows: Iterable[dict[str, object]], fields: Sequence[str]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        "w", newline="", encoding="utf-8", dir=path.parent, delete=False
    ) as handle:
        writer = csv.DictWriter(handle, fieldnames=fields, delimiter="\t", extrasaction="ignore")
        writer.writeheader()
        for row in rows:
            writer.writerow({field: row.get(field, "") for field in fields})
        temporary = Path(handle.name)
    temporary.replace(path)


def git_env() -> dict[str, str]:
    environment = os.environ.copy()
    environment["GIT_TERMINAL_PROMPT"] = "0"
    environment["GCM_INTERACTIVE"] = "Never"
    environment["GIT_OPTIONAL_LOCKS"] = "0"
    return environment


def run_command(
    arguments: Sequence[str],
    *,
    cwd: Path | None = None,
    timeout: int = 60,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        list(arguments),
        cwd=cwd,
        env=git_env(),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )


def git(repo: Path, *arguments: str, timeout: int = 60) -> str:
    result = run_command(["git", "-C", str(repo), *arguments], timeout=timeout)
    if result.returncode != 0:
        message = (result.stderr or result.stdout).strip()
        raise RuntimeError(message or f"git {' '.join(arguments)} failed")
    return result.stdout.strip()


def is_git_repository(path: Path) -> bool:
    if not path.is_dir():
        return False
    result = run_command(["git", "-C", str(path), "rev-parse", "--git-dir"], timeout=20)
    return result.returncode == 0


def resolve_under_base(raw_path: str, base_dir: Path) -> Path:
    path = Path(raw_path).expanduser()
    if not path.is_absolute():
        path = base_dir / path
    resolved = path.resolve()
    base = base_dir.resolve()
    try:
        resolved.relative_to(base)
    except ValueError as error:
        raise ValueError(f"path escapes base directory: {raw_path}") from error
    return resolved


def truncate(text: str, limit: int = 500) -> str:
    compact = re.sub(r"\s+", " ", text).strip()
    if len(compact) <= limit:
        return compact
    return compact[: limit - 1] + "…"


def redact_url(url: str) -> str:
    """Remove embedded HTTP(S) credentials before writing a URL to an artifact."""
    if "://" not in url:
        return url
    parsed = urlsplit(url)
    if parsed.username is None and parsed.password is None:
        return url
    hostname = parsed.hostname or ""
    if parsed.port is not None:
        hostname = f"{hostname}:{parsed.port}"
    return urlunsplit((parsed.scheme, hostname, parsed.path, parsed.query, parsed.fragment))


def has_embedded_credentials(url: str) -> bool:
    if "://" not in url:
        return False
    parsed = urlsplit(url)
    return parsed.username is not None or parsed.password is not None


def markdown_slug(heading: str) -> str:
    value = heading.strip().lower()
    value = re.sub(r"[^\w\- ]", "", value, flags=re.UNICODE)
    value = re.sub(r"\s+", "-", value)
    return re.sub(r"-+", "-", value).strip("-")


def parse_iso8601(value: str) -> bool:
    if not value:
        return False
    candidate = value[:-1] + "+00:00" if value.endswith("Z") else value
    try:
        dt.datetime.fromisoformat(candidate)
    except ValueError:
        return False
    return True
