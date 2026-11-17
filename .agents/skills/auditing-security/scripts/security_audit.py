#!/usr/bin/env python3
"""Initialize and validate a ZZZ security-audit run workspace.

Subcommands:

- ``init``            create a run directory outside the repository
- ``check-ledger``    validate ``coverage-ledger.json``
- ``check-findings``  validate ``findings.json``
- ``summary``         print counts for the report

Standard library only; every path is resolved before use so a relative
``--repo`` cannot silently point somewhere else.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any, Sequence

VERDICTS = {"confirmed", "needs_validation", "rejected"}
SEVERITIES = {"P0", "P1", "P2"}
UNIT_STATES = {
    "planned",
    "in_progress",
    "covered",
    "candidate",
    "deferred",
    "blocked",
    "out_of_scope",
}
OWNED_STATES = {"in_progress", "covered", "candidate"}
REASONED_STATES = {"deferred", "blocked", "out_of_scope"}
RUN_STATUSES = {"complete", "incomplete"}

COVERAGE_ID = re.compile(r"^[a-z0-9]+(?:\.[a-z0-9_-]+)+$")
AGENT_ID = re.compile(r"^[a-z0-9][a-z0-9_-]{0,63}$")
FINDING_ID = re.compile(r"^SEC-[0-9]+$")

BOUNDARY_FIELDS = ("principal", "input", "control", "boundary", "affected", "result")
HYPOTHESIS_FIELDS = ("principal", "input", "control", "boundary", "affected")


class AuditError(Exception):
    """Raised for unusable input, reported as exit code 2."""


def parse_arguments(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    subparsers = parser.add_subparsers(dest="command", required=True)

    init_parser = subparsers.add_parser("init", help="create a run directory outside the repository")
    init_parser.add_argument("--workspace", type=Path, required=True)
    init_parser.add_argument("--repo", type=Path, required=True)
    init_parser.add_argument("--run-id", required=True)

    ledger_parser = subparsers.add_parser("check-ledger", help="validate coverage-ledger.json")
    ledger_parser.add_argument("--ledger", type=Path, required=True)

    findings_parser = subparsers.add_parser("check-findings", help="validate findings.json")
    findings_parser.add_argument("--findings", type=Path, required=True)
    findings_parser.add_argument("--ledger", type=Path)
    findings_parser.add_argument("--repo", type=Path)
    findings_parser.add_argument("--allow-missing", action="store_true")

    summary_parser = subparsers.add_parser("summary", help="print coverage and finding counts")
    summary_parser.add_argument("--findings", type=Path, required=True)
    summary_parser.add_argument("--ledger", type=Path)

    return parser.parse_args(argv)


def load_json(path: Path) -> Any:
    if not path.is_file():
        raise AuditError(f"missing file: {path}")
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        raise AuditError(f"{path}: invalid JSON: {error}") from error


def non_empty(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def validate_ledger(data: Any) -> list[str]:
    findings: list[str] = []
    if not isinstance(data, dict):
        return ["coverage ledger must be a JSON object"]
    if not non_empty(data.get("run_id")):
        findings.append("run_id must be a non-empty string")

    units = data.get("units")
    if not isinstance(units, list) or not units:
        findings.append("units must be a non-empty list (seed at least one unit before checking)")
        return findings

    seen: set[str] = set()
    for index, unit in enumerate(units):
        label = f"units[{index}]"
        if not isinstance(unit, dict):
            findings.append(f"{label}: must be an object")
            continue

        coverage_id = unit.get("coverage_id")
        if not non_empty(coverage_id):
            findings.append(f"{label}: coverage_id is required")
        elif not COVERAGE_ID.match(coverage_id):
            findings.append(
                f"{label}: coverage_id {coverage_id!r} must be lowercase dot-separated segments"
            )
        elif coverage_id in seen:
            findings.append(f"{label}: duplicate coverage_id {coverage_id!r}")
        else:
            seen.add(coverage_id)
            label = f"unit {coverage_id}"

        for field in ("surface", "boundary", "attack_class"):
            if not non_empty(unit.get(field)):
                findings.append(f"{label}: {field} is required")

        state = unit.get("state")
        if state not in UNIT_STATES:
            findings.append(f"{label}: state {state!r} is not one of {sorted(UNIT_STATES)}")
            continue

        owner = unit.get("owner")
        if state in OWNED_STATES:
            if not non_empty(owner):
                findings.append(f"{label}: state {state!r} requires an owner")
            elif not AGENT_ID.match(owner):
                findings.append(f"{label}: owner {owner!r} must match {AGENT_ID.pattern}")
        if state in REASONED_STATES and not non_empty(unit.get("reason")):
            findings.append(f"{label}: state {state!r} requires a reason")

    return findings


def validate_evidence(
    evidence: Any,
    label: str,
    repo: Path | None,
    allow_missing: bool,
) -> list[str]:
    findings: list[str] = []
    if not isinstance(evidence, list) or not evidence:
        return [f"{label}: evidence must be a non-empty list"]

    for index, item in enumerate(evidence):
        item_label = f"{label} evidence[{index}]"
        if not isinstance(item, dict):
            findings.append(f"{item_label}: must be an object")
            continue
        path = item.get("path")
        line = item.get("line")
        if not non_empty(path):
            findings.append(f"{item_label}: path is required")
        elif repo is not None and not allow_missing and not (repo / path).exists():
            findings.append(f"{item_label}: path does not exist in the repository: {path}")
        if not isinstance(line, int) or isinstance(line, bool) or line < 1:
            findings.append(f"{item_label}: line must be a positive integer")

    return findings


def validate_confirmed(
    finding: dict[str, Any],
    label: str,
    repo: Path | None,
    allow_missing: bool,
) -> list[str]:
    findings: list[str] = []

    severity = finding.get("severity")
    if severity not in SEVERITIES:
        findings.append(f"{label}: confirmed needs severity in {sorted(SEVERITIES)}")

    boundary = finding.get("boundary")
    if not isinstance(boundary, dict):
        findings.append(f"{label}: boundary must be an object")
    else:
        for field in BOUNDARY_FIELDS:
            if not non_empty(boundary.get(field)):
                findings.append(f"{label}: boundary.{field} is required for a confirmed finding")

    findings.extend(validate_evidence(finding.get("evidence"), label, repo, allow_missing))

    fix = finding.get("fix")
    if not isinstance(fix, dict) or not non_empty(fix.get("invariant")) or not non_empty(fix.get("change")):
        findings.append(f"{label}: fix needs a non-empty invariant and change")

    verification = finding.get("verification")
    if not isinstance(verification, dict):
        findings.append(f"{label}: verification must be an object")
    else:
        if verification.get("verdict") not in {"confirmed", "rejected", "corrected"}:
            findings.append(f"{label}: verification.verdict must be confirmed, rejected, or corrected")
        if not non_empty(verification.get("notes")):
            findings.append(f"{label}: verification.notes is required")

    found_by = finding.get("found_by")
    verified_by = finding.get("verified_by")
    if not non_empty(found_by):
        findings.append(f"{label}: found_by is required")
    if not non_empty(verified_by):
        findings.append(f"{label}: verified_by is required")
    if non_empty(found_by) and found_by == verified_by:
        findings.append(f"{label}: verified_by must differ from found_by")

    return findings


def validate_findings(
    data: Any,
    ledger: Any | None,
    repo: Path | None,
    allow_missing: bool,
) -> list[str]:
    findings: list[str] = []
    if not isinstance(data, dict):
        return ["findings document must be a JSON object"]

    if not non_empty(data.get("run_id")):
        findings.append("run_id must be a non-empty string")
    if not non_empty(data.get("repo")):
        findings.append("repo must be a non-empty string")

    source_ref = data.get("source_ref")
    if not isinstance(source_ref, dict):
        findings.append("source_ref must be an object")
    else:
        if not non_empty(source_ref.get("commit")):
            findings.append("source_ref.commit must be a non-empty string")
        if not isinstance(source_ref.get("dirty"), bool):
            findings.append("source_ref.dirty must be a boolean")

    run_status = data.get("run_status")
    if run_status not in RUN_STATUSES:
        findings.append(f"run_status must be one of {sorted(RUN_STATUSES)}")
    elif run_status == "incomplete" and not non_empty(data.get("incomplete_reason")):
        findings.append("run_status incomplete requires incomplete_reason")

    records = data.get("findings")
    if not isinstance(records, list):
        return findings + ["findings must be a list"]

    ledger_units = set()
    if isinstance(ledger, dict) and isinstance(ledger.get("units"), list):
        ledger_units = {
            unit["coverage_id"]
            for unit in ledger["units"]
            if isinstance(unit, dict) and non_empty(unit.get("coverage_id"))
        }

    seen: set[str] = set()
    for index, finding in enumerate(records):
        label = f"findings[{index}]"
        if not isinstance(finding, dict):
            findings.append(f"{label}: must be an object")
            continue

        finding_id = finding.get("id")
        if not non_empty(finding_id) or not FINDING_ID.match(finding_id or ""):
            findings.append(f"{label}: id must match {FINDING_ID.pattern}")
        elif finding_id in seen:
            findings.append(f"{label}: duplicate id {finding_id!r}")
        else:
            seen.add(finding_id)
            label = f"finding {finding_id}"

        if not non_empty(finding.get("title")):
            findings.append(f"{label}: title is required")

        status = finding.get("status")
        if status not in VERDICTS:
            findings.append(f"{label}: status must be one of {sorted(VERDICTS)}")
            continue

        coverage_id = finding.get("coverage_id")
        if status == "confirmed":
            if not non_empty(coverage_id):
                findings.append(f"{label}: coverage_id is required for a confirmed finding")
            elif ledger_units and coverage_id not in ledger_units:
                findings.append(f"{label}: coverage_id {coverage_id!r} is not in the ledger")
            findings.extend(validate_confirmed(finding, label, repo, allow_missing))
        elif status == "needs_validation":
            if finding.get("severity") not in (None, ""):
                findings.append(f"{label}: needs_validation must not carry a severity")
            if not non_empty(finding.get("validation_blocker")):
                findings.append(f"{label}: validation_blocker is required")
            boundary = finding.get("boundary")
            if not isinstance(boundary, dict):
                findings.append(f"{label}: boundary must be an object")
            else:
                for field in HYPOTHESIS_FIELDS:
                    if not non_empty(boundary.get(field)):
                        findings.append(f"{label}: boundary.{field} is required for a hypothesis")
        else:
            if finding.get("severity") not in (None, ""):
                findings.append(f"{label}: rejected must not carry a severity")
            if not non_empty(finding.get("rejection_reason")):
                findings.append(f"{label}: rejection_reason is required")

    return findings


def print_findings(command: str, findings: list[str]) -> int:
    if not findings:
        print(f"{command}: PASS")
        return 0
    print(f"{command}: FAIL ({len(findings)} problem(s))")
    for finding in findings:
        print(f"  - {finding}")
    return 1


def is_relative_to(path: Path, parent: Path) -> bool:
    try:
        path.relative_to(parent)
    except ValueError:
        return False
    return True


def command_init(args: argparse.Namespace) -> int:
    repo = args.repo.resolve()
    workspace = args.workspace.expanduser().resolve()
    if not repo.is_dir():
        raise AuditError(f"--repo is not a directory: {repo}")
    if is_relative_to(workspace, repo):
        raise AuditError(f"--workspace must be outside the repository: {workspace}")
    if workspace.exists() and any(workspace.iterdir()):
        raise AuditError(f"--workspace is not empty: {workspace}")

    workspace.mkdir(parents=True, exist_ok=True)
    metadata = {
        "run_id": args.run_id,
        "repo": repo.name,
        "target": str(repo),
        "source_ref": {"commit": "", "dirty": None},
        "profile": "standard",
        "scope_paths": [],
        "run_status": "in_progress",
    }
    ledger = {"run_id": args.run_id, "units": []}
    findings = {
        "run_id": args.run_id,
        "repo": repo.name,
        "source_ref": {"commit": "", "dirty": False},
        "run_status": "incomplete",
        "incomplete_reason": "reconnaissance has not run",
        "findings": [],
    }

    for name, document in (
        ("run-metadata.json", metadata),
        ("coverage-ledger.json", ledger),
        ("findings.json", findings),
    ):
        (workspace / name).write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")

    print(f"created run workspace: {workspace}")
    print("next: fill source_ref in run-metadata.json and seed coverage-ledger.json")
    return 0


def command_check_ledger(args: argparse.Namespace) -> int:
    return print_findings("check-ledger", validate_ledger(load_json(args.ledger)))


def command_check_findings(args: argparse.Namespace) -> int:
    data = load_json(args.findings)
    ledger = load_json(args.ledger) if args.ledger else None
    repo = args.repo.resolve() if args.repo else None
    return print_findings(
        "check-findings",
        validate_findings(data, ledger, repo, args.allow_missing),
    )


def command_summary(args: argparse.Namespace) -> int:
    data = load_json(args.findings)
    records = data.get("findings") if isinstance(data, dict) else None
    if not isinstance(records, list):
        raise AuditError("findings.json has no findings list")

    status_counts: dict[str, int] = {status: 0 for status in sorted(VERDICTS)}
    severity_counts: dict[str, int] = {severity: 0 for severity in sorted(SEVERITIES)}
    for finding in records:
        if not isinstance(finding, dict):
            continue
        status = finding.get("status")
        if status in status_counts:
            status_counts[status] += 1
        severity = finding.get("severity")
        if severity in severity_counts:
            severity_counts[severity] += 1

    print("| status | count |")
    print("| --- | --- |")
    for status, count in status_counts.items():
        print(f"| {status} | {count} |")
    print()
    print("| severity | count |")
    print("| --- | --- |")
    for severity, count in severity_counts.items():
        print(f"| {severity} | {count} |")

    if args.ledger:
        ledger = load_json(args.ledger)
        units = ledger.get("units") if isinstance(ledger, dict) else None
        if isinstance(units, list):
            state_counts: dict[str, int] = {}
            for unit in units:
                if isinstance(unit, dict) and isinstance(unit.get("state"), str):
                    state_counts[unit["state"]] = state_counts.get(unit["state"], 0) + 1
            print()
            print("| unit state | count |")
            print("| --- | --- |")
            for state in sorted(state_counts):
                print(f"| {state} | {state_counts[state]} |")

    run_status = data.get("run_status")
    print()
    print(f"run_status: {run_status}")
    return 0


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_arguments(argv)
    handlers = {
        "init": command_init,
        "check-ledger": command_check_ledger,
        "check-findings": command_check_findings,
        "summary": command_summary,
    }
    try:
        return handlers[args.command](args)
    except AuditError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
