#!/usr/bin/env python3
"""Validate repository-research evidence paths, locators, commits, and metadata."""

from __future__ import annotations

import argparse
import re
from pathlib import Path
from urllib.parse import urlparse

from _common import git, is_git_repository, markdown_slug, parse_iso8601, read_tsv, resolve_under_base

EVIDENCE_KINDS = {"implementation", "test", "documentation", "external-primary", "inference", "unknown"}
CONFIDENCE = {"high", "medium", "low"}
LOCAL_KINDS = {"implementation", "test", "documentation"}
LINE_LOCATOR = re.compile(r"L([1-9]\d*)(?:-L?([1-9]\d*))?$")


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sources", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--base-dir", type=Path, default=Path.cwd())
    return parser.parse_args()


def heading_exists(path: Path, locator: str) -> bool:
    wanted = locator.removeprefix("#").lower()
    try:
        for line in path.read_text(encoding="utf-8", errors="ignore").splitlines():
            match = re.match(r"^#{1,6}\s+(.+?)\s*#*\s*$", line)
            if match and markdown_slug(match.group(1)) == wanted:
                return True
    except OSError:
        return False
    return False


def build_repository_map(rows: list[dict[str, str]], base_dir: Path) -> dict[str, Path]:
    mapping: dict[str, Path] = {}
    for row in rows:
        clone_path = row.get("clone_path", "")
        if not clone_path:
            continue
        try:
            path = resolve_under_base(clone_path, base_dir)
        except ValueError:
            continue
        keys = {
            row.get("id", ""),
            row.get("requested_name", ""),
            path.name,
            clone_path,
        }
        for key in keys:
            if key:
                mapping[key] = path
    return mapping


def validate_local(row: dict[str, str], repo: Path) -> list[str]:
    errors: list[str] = []
    if not is_git_repository(repo):
        return [f"repository checkout is missing or not Git: {repo}"]
    relative = row.get("path_or_endpoint", "").strip()
    if not relative:
        return ["local evidence requires path_or_endpoint"]
    candidate = (repo / relative).resolve()
    try:
        candidate.relative_to(repo.resolve())
    except ValueError:
        return ["evidence path escapes repository checkout"]
    if not candidate.is_file():
        return [f"evidence file does not exist: {relative}"]

    locator = row.get("locator", "").strip()
    if locator == "file":
        pass
    elif locator.startswith("#"):
        if candidate.suffix.lower() not in {".md", ".mdx", ".rst"}:
            errors.append("heading locator is only valid for documentation files")
        elif not heading_exists(candidate, locator):
            errors.append(f"heading locator not found: {locator}")
    else:
        match = LINE_LOCATOR.fullmatch(locator)
        if not match:
            errors.append("locator must be file, #heading, L<number>, or L<start>-L<end>")
        else:
            start = int(match.group(1))
            end = int(match.group(2) or start)
            if end < start:
                errors.append("line locator ends before it starts")
            try:
                line_count = sum(1 for _ in candidate.open(encoding="utf-8", errors="ignore"))
                if end > line_count:
                    errors.append(f"line locator exceeds file length ({line_count})")
            except OSError as error:
                errors.append(str(error))

    commit = row.get("commit_sha", "").strip().lower()
    if not re.fullmatch(r"[0-9a-f]{7,64}", commit):
        errors.append("local evidence requires a 7-64 character hexadecimal commit_sha")
    else:
        try:
            head = git(repo, "rev-parse", "HEAD").lower()
            if not head.startswith(commit) and not commit.startswith(head):
                errors.append(f"commit_sha does not match checkout HEAD {head[:12]}")
        except RuntimeError as error:
            errors.append(str(error))
    return errors


def validate_external(row: dict[str, str]) -> list[str]:
    errors: list[str] = []
    endpoint = row.get("path_or_endpoint", "").strip()
    parsed = urlparse(endpoint)
    if parsed.scheme not in {"http", "https"} or not parsed.netloc:
        errors.append("external-primary evidence requires an HTTP(S) URL")
    if not parse_iso8601(row.get("checked_at", "").strip()):
        errors.append("external-primary evidence requires an ISO-8601 checked_at timestamp")
    return errors


def main() -> int:
    arguments = parse_arguments()
    base_dir = arguments.base_dir.resolve()
    repositories = build_repository_map(read_tsv(arguments.sources), base_dir)
    rows = read_tsv(arguments.evidence)
    seen: set[str] = set()
    inference_rows: list[tuple[int, dict[str, str]]] = []
    errors: list[str] = []
    warnings: list[str] = []

    for number, row in enumerate(rows, start=2):
        prefix = f"row {number}"
        claim_id = row.get("claim_id", "").strip()
        if not claim_id:
            errors.append(f"{prefix}: claim_id is required")
        elif claim_id in seen:
            errors.append(f"{prefix}: duplicate claim_id {claim_id}")
        seen.add(claim_id)
        if not row.get("claim", "").strip():
            errors.append(f"{prefix}: claim is required")

        kind = row.get("evidence_kind", "").strip()
        if kind not in EVIDENCE_KINDS:
            errors.append(f"{prefix}: invalid evidence_kind {kind!r}")
            continue
        confidence = row.get("confidence", "").strip()
        if confidence not in CONFIDENCE:
            errors.append(f"{prefix}: confidence must be high, medium, or low")

        if kind in LOCAL_KINDS:
            repository = row.get("repository", "").strip()
            repo = repositories.get(repository)
            if repo is None:
                errors.append(f"{prefix}: unknown repository {repository!r}")
            else:
                errors.extend(f"{prefix}: {message}" for message in validate_local(row, repo))
        elif kind == "external-primary":
            errors.extend(f"{prefix}: {message}" for message in validate_external(row))
        elif kind in {"inference", "unknown"}:
            if not row.get("notes", "").strip():
                errors.append(f"{prefix}: {kind} evidence requires explanatory notes")
            if kind == "inference":
                inference_rows.append((number, row))

        if kind == "documentation" and confidence == "high":
            warnings.append(f"{prefix}: documentation-only evidence is rarely high confidence for behavior")

    for number, row in inference_rows:
        claim_id = row.get("claim_id", "").strip()
        notes = row.get("notes", "")
        supporting_ids = [candidate for candidate in seen if candidate != claim_id and candidate in notes]
        if not supporting_ids:
            warnings.append(f"row {number}: inference notes should identify supporting claim IDs")

    for message in errors:
        print(f"ERROR\t{message}")
    for message in warnings:
        print(f"WARN\t{message}")
    print(f"claims={len(rows)} errors={len(errors)} warnings={len(warnings)}")
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
