#!/usr/bin/env python3
"""Tests for security_audit.py. Run: python3 -m unittest test_security_audit"""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

import security_audit as audit  # noqa: E402


def boundary(**overrides: str) -> dict[str, str]:
    values = {
        "principal": "untrusted project content",
        "input": "a skill file under .agents/skills/",
        "control": "worktree trust check before loading project skills",
        "boundary": "project trust",
        "affected": "the agent's system prompt and tool permissions",
        "result": "skill instructions reach the prompt for an untrusted project",
    }
    values.update(overrides)
    return values


def ledger(units: list[dict[str, object]] | None = None) -> dict[str, object]:
    return {
        "run_id": "run-1",
        "units": units
        if units is not None
        else [
            {
                "coverage_id": "agent.skills.trust",
                "surface": "agent skill loading",
                "boundary": "worktree trust",
                "attack_class": "untrusted project content",
                "state": "planned",
            }
        ],
    }


def unit(**overrides: object) -> dict[str, object]:
    values: dict[str, object] = {
        "coverage_id": "agent.skills.trust",
        "surface": "agent skill loading",
        "boundary": "worktree trust",
        "attack_class": "untrusted project content",
        "state": "planned",
    }
    values.update(overrides)
    return values


def finding(**overrides: object) -> dict[str, object]:
    values: dict[str, object] = {
        "id": "SEC-1",
        "status": "confirmed",
        "title": "Project skills load from an untrusted worktree",
        "severity": "P0",
        "coverage_id": "agent.skills.trust",
        "found_by": "hunter-a",
        "verified_by": "verifier-b",
        "boundary": boundary(),
        "evidence": [{"path": "STATE.md", "line": 1}],
        "fix": {"invariant": "skills load only from trusted worktrees", "change": "check trust"},
        "verification": {"verdict": "confirmed", "notes": "re-read the loader"},
    }
    values.update(overrides)
    return values


def document(**overrides: object) -> dict[str, object]:
    values: dict[str, object] = {
        "run_id": "run-1",
        "repo": "zzz",
        "source_ref": {"commit": "abc1234", "dirty": False},
        "run_status": "complete",
        "findings": [finding()],
    }
    values.update(overrides)
    return values


class LedgerTests(unittest.TestCase):
    def test_accepts_a_seeded_ledger(self) -> None:
        self.assertEqual(audit.validate_ledger(ledger()), [])

    def test_rejects_an_empty_ledger(self) -> None:
        problems = audit.validate_ledger(ledger([]))
        self.assertTrue(any("non-empty list" in problem for problem in problems))

    def test_rejects_a_duplicate_coverage_id(self) -> None:
        problems = audit.validate_ledger(ledger([unit(), unit()]))
        self.assertTrue(any("duplicate coverage_id" in problem for problem in problems))

    def test_rejects_a_malformed_coverage_id(self) -> None:
        problems = audit.validate_ledger(ledger([unit(coverage_id="Agent Skills")]))
        self.assertTrue(any("lowercase dot-separated" in problem for problem in problems))

    def test_requires_an_owner_once_work_starts(self) -> None:
        problems = audit.validate_ledger(ledger([unit(state="in_progress")]))
        self.assertTrue(any("requires an owner" in problem for problem in problems))

    def test_rejects_an_unsafe_owner_id(self) -> None:
        problems = audit.validate_ledger(ledger([unit(state="in_progress", owner="../../etc")]))
        self.assertTrue(any("must match" in problem for problem in problems))

    def test_requires_a_reason_for_deferred_units(self) -> None:
        problems = audit.validate_ledger(ledger([unit(state="deferred")]))
        self.assertTrue(any("requires a reason" in problem for problem in problems))

    def test_state_must_be_known(self) -> None:
        problems = audit.validate_ledger(ledger([unit(state="done")]))
        self.assertTrue(any("is not one of" in problem for problem in problems))


class FindingsTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.repo = Path(self.temporary.name)
        (self.repo / "STATE.md").write_text("evidence\n", encoding="utf-8")

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def check(self, data: dict[str, object], allow_missing: bool = False) -> list[str]:
        return audit.validate_findings(data, ledger(), self.repo, allow_missing)

    def test_accepts_a_confirmed_finding(self) -> None:
        self.assertEqual(self.check(document()), [])

    def test_requires_every_boundary_field(self) -> None:
        incomplete = boundary()
        del incomplete["result"]
        problems = self.check(document(findings=[finding(boundary=incomplete)]))
        self.assertTrue(any("boundary.result is required" in problem for problem in problems))

    def test_rejects_an_unknown_severity(self) -> None:
        problems = self.check(document(findings=[finding(severity="high")]))
        self.assertTrue(any("severity in" in problem for problem in problems))

    def test_rejects_the_same_agent_finding_and_verifying(self) -> None:
        problems = self.check(document(findings=[finding(verified_by="hunter-a")]))
        self.assertTrue(any("must differ from found_by" in problem for problem in problems))

    def test_rejects_evidence_outside_the_repository(self) -> None:
        problems = self.check(document(findings=[finding(evidence=[{"path": "NOPE.md", "line": 3}])]))
        self.assertTrue(any("does not exist" in problem for problem in problems))

    def test_allow_missing_downgrades_absent_evidence(self) -> None:
        problems = self.check(
            document(findings=[finding(evidence=[{"path": "NOPE.md", "line": 3}])]),
            allow_missing=True,
        )
        self.assertEqual(problems, [])

    def test_confirmed_needs_a_ledger_unit(self) -> None:
        problems = self.check(document(findings=[finding(coverage_id="other.unit.here")]))
        self.assertTrue(any("is not in the ledger" in problem for problem in problems))

    def test_needs_validation_carries_no_severity(self) -> None:
        problems = self.check(
            document(
                findings=[
                    finding(
                        status="needs_validation",
                        validation_blocker="the packaged entitlement is not in source",
                    )
                ]
            )
        )
        self.assertTrue(any("must not carry a severity" in problem for problem in problems))

    def test_needs_validation_keeps_the_hypothesis(self) -> None:
        incomplete = boundary()
        del incomplete["control"]
        problems = self.check(
            document(
                findings=[
                    finding(
                        status="needs_validation",
                        boundary=incomplete,
                        validation_blocker="control lives in the OS sandbox profile",
                    )
                ]
            )
        )
        self.assertTrue(any("boundary.control is required" in problem for problem in problems))

    def test_needs_validation_needs_a_blocker(self) -> None:
        problems = self.check(document(findings=[finding(status="needs_validation", severity=None)]))
        self.assertTrue(any("validation_blocker is required" in problem for problem in problems))

    def test_rejected_needs_a_reason(self) -> None:
        problems = self.check(document(findings=[finding(status="rejected", severity=None)]))
        self.assertTrue(any("rejection_reason is required" in problem for problem in problems))

    def test_ids_are_unique(self) -> None:
        problems = self.check(document(findings=[finding(), finding()]))
        self.assertTrue(any("duplicate id" in problem for problem in problems))

    def test_incomplete_runs_state_a_reason(self) -> None:
        problems = self.check(document(run_status="incomplete"))
        self.assertTrue(any("incomplete_reason" in problem for problem in problems))

    def test_dirty_must_be_boolean(self) -> None:
        problems = self.check(document(source_ref={"commit": "abc1234", "dirty": "no"}))
        self.assertTrue(any("dirty must be a boolean" in problem for problem in problems))


class CommandTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.repo = self.root / "repo"
        self.repo.mkdir()
        (self.repo / "STATE.md").write_text("evidence\n", encoding="utf-8")

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def write(self, name: str, data: object) -> Path:
        path = self.root / name
        path.write_text(json.dumps(data), encoding="utf-8")
        return path

    def test_init_writes_the_three_shared_files(self) -> None:
        workspace = self.root / "run-1"
        code = audit.main(
            ["init", "--workspace", str(workspace), "--repo", str(self.repo), "--run-id", "run-1"]
        )
        self.assertEqual(code, 0)
        for name in ("run-metadata.json", "coverage-ledger.json", "findings.json"):
            self.assertTrue((workspace / name).is_file(), name)

    def test_init_refuses_a_workspace_inside_the_repository(self) -> None:
        code = audit.main(
            ["init", "--workspace", str(self.repo / "audit"), "--repo", str(self.repo), "--run-id", "run-1"]
        )
        self.assertEqual(code, 2)

    def test_init_refuses_a_non_empty_workspace(self) -> None:
        workspace = self.root / "run-1"
        workspace.mkdir()
        (workspace / "keep.txt").write_text("existing\n", encoding="utf-8")
        code = audit.main(
            ["init", "--workspace", str(workspace), "--repo", str(self.repo), "--run-id", "run-1"]
        )
        self.assertEqual(code, 2)
        self.assertTrue((workspace / "keep.txt").is_file())

    def test_check_findings_returns_one_on_failure(self) -> None:
        findings_path = self.write("findings.json", document(findings=[finding(severity="P9")]))
        ledger_path = self.write("coverage-ledger.json", ledger())
        code = audit.main(
            [
                "check-findings",
                "--findings",
                str(findings_path),
                "--ledger",
                str(ledger_path),
                "--repo",
                str(self.repo),
            ]
        )
        self.assertEqual(code, 1)

    def test_check_findings_returns_zero_on_success(self) -> None:
        findings_path = self.write("findings.json", document())
        ledger_path = self.write("coverage-ledger.json", ledger())
        code = audit.main(
            [
                "check-findings",
                "--findings",
                str(findings_path),
                "--ledger",
                str(ledger_path),
                "--repo",
                str(self.repo),
            ]
        )
        self.assertEqual(code, 0)

    def test_summary_counts_statuses(self) -> None:
        findings_path = self.write(
            "findings.json",
            document(
                findings=[
                    finding(),
                    finding(id="SEC-2", status="rejected", severity=None, rejection_reason="control holds"),
                ]
            ),
        )
        self.assertEqual(audit.main(["summary", "--findings", str(findings_path)]), 0)

    def test_missing_file_is_a_usage_error(self) -> None:
        self.assertEqual(audit.main(["check-ledger", "--ledger", str(self.root / "absent.json")]), 2)


if __name__ == "__main__":
    unittest.main()
