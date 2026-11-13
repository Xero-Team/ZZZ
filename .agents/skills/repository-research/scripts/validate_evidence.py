#!/usr/bin/env python3
"""Validate repository-research evidence paths, locators, commits, and metadata."""

from __future__ import annotations

import argparse
import re
from pathlib import Path
from typing import Sequence
from urllib.parse import urlparse

from _common import (
    git,
    is_git_repository,
    markdown_slug,
    parse_iso8601,
    read_tsv,
    resolve_under_base,
    run_command,
)

EVIDENCE_KINDS = {"implementation", "test", "documentation", "external-primary", "inference", "unknown"}
CONFIDENCE = {"high", "medium", "low"}
LOCAL_KINDS = {"implementation", "test", "documentation"}
LINE_LOCATOR = re.compile(r"L([1-9]\d*)(?:-L?([1-9]\d*))?$")
CLAIM_PREFIX = re.compile(r"([A-Za-z]+)-\d+$")
CITATION = re.compile(r"\[([A-Za-z]+-\d+)\]")
LINEAGE_COUNTS = ("common_paths", "byte_identical", "changed_common_paths", "only_a", "only_b")


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sources", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--report", type=Path)
    parser.add_argument("--lineage", type=Path)
    parser.add_argument("--base-dir", type=Path, default=Path.cwd())
    parser.add_argument(
        "--allow-missing-checkout",
        action="store_true",
        help="report a missing checkout as a warning instead of skipping the whole ledger",
    )
    return parser.parse_args()


def heading_exists(text: str, locator: str) -> bool:
    wanted = locator.removeprefix("#").lower()
    for line in text.splitlines():
        match = re.match(r"^#{1,6}\s+(.+?)\s*#*\s*$", line)
        if match and markdown_slug(match.group(1)) == wanted:
            return True
    return False


def read_at_commit(repo: Path, commit: str, relative: str) -> str | None:
    result = run_command(["git", "-C", str(repo), "show", f"{commit}:{relative}"], timeout=20)
    if result.returncode != 0:
        return None
    return result.stdout


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


def check_commit(repo: Path, commit: str) -> str | None:
    try:
        head = git(repo, "rev-parse", "HEAD").lower()
    except RuntimeError as error:
        return str(error)
    if head.startswith(commit) or commit.startswith(head):
        return None
    result = run_command(["git", "-C", str(repo), "merge-base", "--is-ancestor", commit, "HEAD"], timeout=20)
    if result.returncode == 0:
        return None
    if result.returncode == 1:
        return f"commit_sha is not an ancestor of checkout HEAD {head[:12]}"
    return f"commit_sha is not present in the checkout: {commit}"


def validate_local(row: dict[str, str], repo: Path) -> tuple[list[str], bool, bool]:
    if not is_git_repository(repo):
        return [], True, False
    errors: list[str] = []
    relative = row.get("path_or_endpoint", "").strip()
    if not relative:
        return ["local evidence requires path_or_endpoint"], False, False
    candidate = (repo / relative).resolve()
    try:
        candidate.relative_to(repo.resolve())
    except ValueError:
        return ["evidence path escapes repository checkout"], False, False

    commit = row.get("commit_sha", "").strip().lower()
    if not re.fullmatch(r"[0-9a-f]{7,64}", commit):
        return ["local evidence requires a 7-64 character hexadecimal commit_sha"], False, False
    commit_error = check_commit(repo, commit)
    if commit_error:
        return [commit_error], False, False
    try:
        head = git(repo, "rev-parse", "HEAD").lower()
    except RuntimeError as error:
        return [str(error)], False, False
    at_head = head.startswith(commit) or commit.startswith(head)

    if at_head:
        if not candidate.is_file():
            return [f"evidence file does not exist: {relative}"], False, False
        text = candidate.read_text(encoding="utf-8", errors="ignore")
    else:
        text = read_at_commit(repo, commit, relative)
        if text is None:
            return [f"evidence file does not exist at {commit[:12]}: {relative}"], False, True
    suffix = Path(relative).suffix.lower()

    locator = row.get("locator", "").strip()
    if locator == "file":
        pass
    elif locator.startswith("#"):
        if suffix not in {".md", ".mdx", ".rst"}:
            errors.append("heading locator is only valid for documentation files")
        elif not heading_exists(text, locator):
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
            else:
                line_count = len(text.splitlines())
                if end > line_count:
                    errors.append(f"line locator exceeds file length ({line_count} at {commit[:12]})")
    return errors, False, not at_head


def validate_external(row: dict[str, str]) -> list[str]:
    errors: list[str] = []
    endpoint = row.get("path_or_endpoint", "").strip()
    parsed = urlparse(endpoint)
    if parsed.scheme not in {"http", "https"} or not parsed.netloc:
        errors.append("external-primary evidence requires an HTTP(S) URL")
    if not parse_iso8601(row.get("checked_at", "").strip()):
        errors.append("external-primary evidence requires an ISO-8601 checked_at timestamp")
    return errors


def check_claim_prefixes(rows: list[dict[str, str]]) -> list[str]:
    owners: dict[str, set[str]] = {}
    for row in rows:
        match = CLAIM_PREFIX.fullmatch(row.get("claim_id", "").strip())
        if not match:
            continue
        owners.setdefault(match.group(1).upper(), set()).add(row.get("repository", "").strip())
    return [
        f"claim prefix {prefix}- maps to several repositories: {', '.join(sorted(repositories))}"
        for prefix, repositories in sorted(owners.items())
        if len(repositories) > 1
    ]


def check_report(path: Path, claim_ids: Sequence[str]) -> tuple[list[str], list[str]]:
    if not path.is_file():
        return [f"report does not exist: {path}"], []
    text = path.read_text(encoding="utf-8", errors="ignore")
    cited = set(CITATION.findall(text))
    known = set(claim_ids)
    errors = [
        f"report cites {claim_id}, which the evidence ledger does not define"
        for claim_id in sorted(cited - known)
    ]
    uncited = sorted(claim_id for claim_id in known - cited if claim_id)
    warnings: list[str] = []
    if uncited:
        preview = ", ".join(uncited[:8]) + (", ..." if len(uncited) > 8 else "")
        warnings.append(f"{len(uncited)} evidence claims are never cited in the report: {preview}")
    return errors, warnings


def check_lineage(
    path: Path, repositories: dict[str, Path]
) -> tuple[list[str], list[str], int]:
    if not path.is_file():
        return [f"lineage ledger does not exist: {path}"], [], 0
    rows = read_tsv(path)
    errors: list[str] = []
    warnings: list[str] = []
    unpinned = 0
    for number, row in enumerate(rows, start=2):
        prefix = f"lineage row {number}"
        counts: dict[str, int] = {}
        for field in LINEAGE_COUNTS:
            raw = row.get(field, "").strip()
            if not raw.isdigit():
                errors.append(f"{prefix}: {field} must be a non-negative integer")
                continue
            counts[field] = int(raw)
        for side in ("repository_a", "repository_b"):
            identifier = row.get(side, "").strip()
            if identifier not in repositories:
                errors.append(f"{prefix}: unknown repository {identifier!r} in {side}")
        if (
            len(counts) == len(LINEAGE_COUNTS)
            and counts["common_paths"] != counts["byte_identical"] + counts["changed_common_paths"]
        ):
            errors.append(f"{prefix}: common_paths must equal byte_identical + changed_common_paths")
        head_a = row.get("head_a", "").strip().lower()
        head_b = row.get("head_b", "").strip().lower()
        if not (head_a and head_b):
            unpinned += 1
            continue
        for side, head, key in (("head_a", head_a, "repository_a"), ("head_b", head_b, "repository_b")):
            repo = repositories.get(row.get(key, "").strip())
            if repo is None or not is_git_repository(repo):
                continue
            try:
                current = git(repo, "rev-parse", "HEAD").lower()
            except RuntimeError as error:
                warnings.append(f"{prefix}: {error}")
                continue
            if not (current.startswith(head) or head.startswith(current)):
                warnings.append(f"{prefix}: {side} matches no checkout HEAD; the lineage row is stale")
    if unpinned:
        warnings.append(f"{unpinned} lineage rows lack head pins; they cannot be re-checked against checkouts")
    return errors, warnings, len(rows)


def main() -> int:
    arguments = parse_arguments()
    missing_inputs = [str(path) for path in (arguments.sources, arguments.evidence) if not path.is_file()]
    if missing_inputs:
        print(f"ERROR\tinput ledger does not exist: {', '.join(missing_inputs)}")
        return 1
    base_dir = arguments.base_dir.resolve()
    repositories = build_repository_map(read_tsv(arguments.sources), base_dir)
    rows = read_tsv(arguments.evidence)
    seen: set[str] = set()
    inference_rows: list[tuple[int, dict[str, str]]] = []
    errors: list[str] = []
    warnings: list[str] = []
    missing_checkouts = 0
    stale_pins = 0

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
                row_errors, missing, stale = validate_local(row, repo)
                if missing and arguments.allow_missing_checkout:
                    missing_checkouts += 1
                    warnings.append(f"{prefix}: checkout unavailable, local checks skipped: {repo}")
                elif missing:
                    errors.append(f"{prefix}: repository checkout is missing or not Git: {repo}")
                else:
                    stale_pins += stale
                    errors.extend(f"{prefix}: {message}" for message in row_errors)
        elif kind == "external-primary":
            errors.extend(f"{prefix}: {message}" for message in validate_external(row))
        elif kind in {"inference", "unknown"}:
            if not row.get("notes", "").strip():
                errors.append(f"{prefix}: {kind} evidence requires explanatory notes")
            if kind == "inference":
                inference_rows.append((number, row))

        if kind == "documentation" and confidence == "high":
            warnings.append(f"{prefix}: documentation-only evidence is rarely high confidence for behavior")
        checked_at = row.get("checked_at", "").strip()
        if checked_at and not parse_iso8601(checked_at):
            errors.append(f"{prefix}: checked_at must be ISO-8601 when present")

    for number, row in inference_rows:
        claim_id = row.get("claim_id", "").strip()
        notes = row.get("notes", "")
        supporting_ids = [candidate for candidate in seen if candidate != claim_id and candidate in notes]
        if not supporting_ids:
            warnings.append(f"row {number}: inference notes should identify supporting claim IDs")

    errors.extend(check_claim_prefixes(rows))
    lineage_pairs = 0
    if arguments.lineage is not None:
        lineage_errors, lineage_warnings, lineage_pairs = check_lineage(arguments.lineage, repositories)
        errors.extend(lineage_errors)
        warnings.extend(lineage_warnings)
    if arguments.report is not None:
        report_errors, report_warnings = check_report(
            arguments.report, [row.get("claim_id", "").strip() for row in rows]
        )
        errors.extend(report_errors)
        warnings.extend(report_warnings)

    if stale_pins:
        warnings.insert(
            0,
            f"{stale_pins} local claims are pinned to a revision older than checkout HEAD; "
            "re-read them before reusing the conclusions",
        )

    for message in errors:
        print(f"ERROR\t{message}")
    for message in warnings:
        print(f"WARN\t{message}")
    print(
        f"claims={len(rows)} errors={len(errors)} warnings={len(warnings)} "
        f"missing_checkouts={missing_checkouts} stale_pins={stale_pins} "
        f"lineage_pairs={lineage_pairs}"
    )
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
