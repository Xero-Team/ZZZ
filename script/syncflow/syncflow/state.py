from __future__ import annotations

import json
import re
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

import pyjson5

from syncflow.git import (
    checkout,
    current_branch,
    create_branch,
    delete_local_branch,
    delete_remote_branch,
    ensure_clean_worktree,
    fetch_branch,
    local_branch_exists,
    merge_no_ff,
    merge_base,
    remote_branch_exists,
    ref_exists,
    rev_parse,
)

from syncflow.models import (
    DEBATE_STATUSES,
    DEBATE_TRIGGER_REASONS,
    DECISION_ACTIONS,
    DECISION_TARGET_TYPES,
    DISAGREEMENT_LEVELS,
    REPORT_STATUSES,
    RESOLUTION_STATUSES,
    REVIEW_STATUSES,
    RUN_PHASE_SEQUENCE,
    RUN_PHASES,
    RUN_STATUSES,
    SCHEMA_VERSION,
    SyncflowError,
    commit_rating,
    needs_challenger_review,
    phase_index,
    require_enum,
    score_breakdown,
)


def utc_now() -> str:
    return datetime.now(UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def new_run_id() -> str:
    return datetime.now(UTC).replace(microsecond=0).strftime("%Y-%m-%dT%H-%M-%SZ")


def default_sync_branch_name() -> str:
    return datetime.now(UTC).strftime("sync/upstream-%Y-%m-%d")


IDENTIFIER_RE = re.compile(r"^[A-Za-z_$][A-Za-z0-9_$]*$")


def _read_comment_preamble(text: str) -> str:
    if not text.strip():
        return ""
    lines = text.splitlines()
    preamble_lines: list[str] = []
    for line in lines:
        if line.lstrip().startswith(("{", "[")):
            break
        preamble_lines.append(line)
    preamble = "\n".join(preamble_lines).rstrip()
    return preamble + "\n\n" if preamble else ""


def _default_comment_preamble(path: Path) -> str:
    suffix = str(path).replace("\\", "/")
    if suffix.endswith("/.sync/state.json5") or suffix.endswith(".sync/state.json5"):
        return "// syncflow managed state file\n// update through `uv run python -m syncflow ...` only\n\n"
    if suffix.endswith("archive/index.json5"):
        return "// syncflow managed archive summary index\n// historical runs are retained here as summaries\n\n"
    if suffix.endswith("session.json5"):
        return "// syncflow managed run session\n// phase and lifecycle state for one run\n\n"
    if suffix.endswith("reports/index.json5"):
        return "// syncflow managed report index\n// links reports, decisions, and debates\n\n"
    if suffix.endswith("/index.json5"):
        return "// syncflow managed run index\n// object links for one run\n\n"
    if "/commits/" in suffix:
        return "// syncflow managed commit record\n\n"
    if "/decisions/" in suffix:
        return "// syncflow managed decision record\n\n"
    if "/debates/" in suffix:
        return "// syncflow managed debate record\n\n"
    if suffix.endswith("summary.json5"):
        return "// syncflow managed final summary\n\n"
    return "// syncflow managed JSON5 file\n\n"


def _json5_scalar(value: Any) -> str:
    if value is None:
        return "null"
    if value is True:
        return "true"
    if value is False:
        return "false"
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False)
    if isinstance(value, (int, float)):
        return json.dumps(value, ensure_ascii=False)
    raise SyncflowError("unsupported_value", f"Unsupported scalar type: {type(value)!r}")


def _json5_key(key: str) -> str:
    if IDENTIFIER_RE.match(key):
        return key
    return json.dumps(key, ensure_ascii=False)


def _encode_json5(value: Any, indent: int = 0) -> str:
    pad = " " * indent
    next_pad = " " * (indent + 2)
    if isinstance(value, dict):
        if not value:
            return "{}"
        parts = [
            f"{next_pad}{_json5_key(str(key))}: {_encode_json5(inner_value, indent + 2)},"
            for key, inner_value in value.items()
        ]
        return "{\n" + "\n".join(parts) + f"\n{pad}" + "}"
    if isinstance(value, list):
        if not value:
            return "[]"
        parts = [f"{next_pad}{_encode_json5(item, indent + 2)}," for item in value]
        return "[\n" + "\n".join(parts) + f"\n{pad}]"
    return _json5_scalar(value)


def read_json5_like(path: Path) -> dict[str, Any]:
    if not path.exists():
        raise SyncflowError("missing_file", f"Missing file: {path}")
    text = path.read_text(encoding="utf-8")
    try:
        decoded = pyjson5.decode(text)
    except Exception as error:  # noqa: BLE001
        raise SyncflowError("invalid_json5", f"Failed to parse JSON5 file: {path}") from error
    if not isinstance(decoded, dict):
        raise SyncflowError("invalid_json5_root", f"Expected object root in file: {path}")
    return decoded


def write_json5_like(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    existing_preamble = ""
    if path.exists():
        existing_preamble = _read_comment_preamble(path.read_text(encoding="utf-8"))
    preamble = existing_preamble or _default_comment_preamble(path)
    path.write_text(preamble + _encode_json5(payload) + "\n", encoding="utf-8")


class SyncStateStore:
    def __init__(self, repo_root: Path) -> None:
        self.repo_root = repo_root
        self.sync_root = repo_root / ".sync"
        self.runs_root = self.sync_root / "runs"
        self.archive_root = self.sync_root / "archive"
        self.state_path = self.sync_root / "state.json5"
        self.archive_index_path = self.archive_root / "index.json5"

    def ensure_base_layout(self) -> None:
        self.runs_root.mkdir(parents=True, exist_ok=True)
        self.archive_root.mkdir(parents=True, exist_ok=True)
        now = utc_now()
        if not self.state_path.exists():
            write_json5_like(
                self.state_path,
                {
                    "schema_version": SCHEMA_VERSION,
                    "active_run_id": None,
                    "last_completed_at": None,
                    "last_upstream_head": None,
                    "last_absorbed_upstream_commit": None,
                    "last_sync_branch": None,
                    "last_sync_merge_commit": None,
                    "policy_version": "v1",
                    "created_at": now,
                    "updated_at": now,
                },
            )
        if not self.archive_index_path.exists():
            write_json5_like(
                self.archive_index_path,
                {
                    "schema_version": SCHEMA_VERSION,
                    "runs": [],
                    "created_at": now,
                    "updated_at": now,
                },
            )

    def load_state(self) -> dict[str, Any]:
        self.ensure_base_layout()
        return read_json5_like(self.state_path)

    def save_state(self, payload: dict[str, Any]) -> None:
        payload["updated_at"] = utc_now()
        write_json5_like(self.state_path, payload)

    def load_archive_index(self) -> dict[str, Any]:
        self.ensure_base_layout()
        return read_json5_like(self.archive_index_path)

    def save_archive_index(self, payload: dict[str, Any]) -> None:
        payload["updated_at"] = utc_now()
        write_json5_like(self.archive_index_path, payload)

    def run_root(self, run_id: str) -> Path:
        return self.runs_root / run_id

    def run_file(self, run_id: str, relative_path: str) -> Path:
        return self.run_root(run_id) / relative_path

    def load_run_file(self, run_id: str, relative_path: str) -> dict[str, Any]:
        return read_json5_like(self.run_file(run_id, relative_path))

    def save_run_file(
        self, run_id: str, relative_path: str, payload: dict[str, Any]
    ) -> None:
        payload["updated_at"] = utc_now()
        write_json5_like(self.run_file(run_id, relative_path), payload)

    def require_active_run(self, run_id: str) -> dict[str, Any]:
        session = self.load_run_file(run_id, "session.json5")
        if session["status"] != "active":
            raise SyncflowError("run_not_active", f"Run is not active: {run_id}")
        return session

    def save_session(self, run_id: str, payload: dict[str, Any]) -> None:
        self.save_run_file(run_id, "session.json5", payload)

    def maybe_advance_phase(self, run_id: str, target_phase: str) -> None:
        require_enum("target_phase", target_phase, RUN_PHASES)
        session = self.require_active_run(run_id)
        if phase_index(target_phase) > phase_index(session["phase"]):
            session["phase"] = target_phase
            self.save_session(run_id, session)

    def require_no_active_run(self) -> None:
        state = self.load_state()
        active_run_id = state.get("active_run_id")
        if active_run_id is not None:
            raise SyncflowError(
                "active_run_exists", f"Active run exists: {active_run_id}"
            )

    def preflight(
        self,
        *,
        base_branch: str,
        upstream_remote: str,
        upstream_branch: str,
        sync_branch: str | None,
    ) -> dict[str, Any]:
        self.require_no_active_run()
        ensure_clean_worktree(self.repo_root, operation="preflight")
        if not ref_exists(base_branch, repo_root=self.repo_root):
            raise SyncflowError(
                "base_branch_not_found", f"Unknown base branch: {base_branch}"
            )

        fetch_branch(upstream_remote, upstream_branch, repo_root=self.repo_root)
        upstream_ref = f"{upstream_remote}/{upstream_branch}"
        if not ref_exists(upstream_ref, repo_root=self.repo_root):
            raise SyncflowError(
                "upstream_ref_not_found", f"Unknown upstream ref: {upstream_ref}"
            )

        resolved_sync_branch = sync_branch or default_sync_branch_name()
        if local_branch_exists(resolved_sync_branch, repo_root=self.repo_root):
            raise SyncflowError(
                "sync_branch_exists_local",
                f"Sync branch already exists locally: {resolved_sync_branch}",
            )
        if remote_branch_exists("origin", resolved_sync_branch, repo_root=self.repo_root):
            raise SyncflowError(
                "sync_branch_exists_remote",
                f"Sync branch already exists on origin: {resolved_sync_branch}",
            )

        base_head = rev_parse(base_branch, repo_root=self.repo_root)
        upstream_head = rev_parse(upstream_ref, repo_root=self.repo_root)
        merge_base_sha = merge_base(base_branch, upstream_ref, repo_root=self.repo_root)
        return {
            "ready": True,
            "base_branch": base_branch,
            "base_head": base_head,
            "upstream_remote": upstream_remote,
            "upstream_branch": upstream_branch,
            "upstream_ref": upstream_ref,
            "upstream_head": upstream_head,
            "merge_base": merge_base_sha,
            "sync_branch": resolved_sync_branch,
            "current_branch": current_branch(self.repo_root),
        }

    def start_run(
        self,
        *,
        base_branch: str,
        upstream_remote: str,
        upstream_branch: str,
        sync_branch: str | None,
        run_id: str | None,
    ) -> dict[str, Any]:
        preflight_result = self.preflight(
            base_branch=base_branch,
            upstream_remote=upstream_remote,
            upstream_branch=upstream_branch,
            sync_branch=sync_branch,
        )
        resolved_sync_branch = str(preflight_result["sync_branch"])
        checkout(base_branch, repo_root=self.repo_root)
        create_branch(
            resolved_sync_branch,
            base_branch,
            repo_root=self.repo_root,
        )
        init_result = self.init_run(
            base_branch=base_branch,
            sync_branch=resolved_sync_branch,
            upstream_head=str(preflight_result["upstream_head"]),
            merge_base=str(preflight_result["merge_base"]),
            run_id=run_id,
            upstream_remote=upstream_remote,
            upstream_branch=upstream_branch,
            base_head=str(preflight_result["base_head"]),
            started_from_branch=str(preflight_result["current_branch"]),
        )
        return {
            **init_result,
            "preflight": preflight_result,
            "created_sync_branch": resolved_sync_branch,
        }

    def init_run(
        self,
        *,
        base_branch: str,
        sync_branch: str,
        upstream_head: str,
        merge_base: str,
        run_id: str | None = None,
        upstream_remote: str = "upstream",
        upstream_branch: str = "main",
        base_head: str | None = None,
        started_from_branch: str | None = None,
    ) -> dict[str, Any]:
        state = self.load_state()
        if state["active_run_id"] is not None:
            raise SyncflowError(
                "active_run_exists", f"Active run exists: {state['active_run_id']}"
            )

        resolved_run_id = run_id or new_run_id()
        run_root = self.run_root(resolved_run_id)
        if run_root.exists():
            raise SyncflowError(
                "run_exists", f"Run directory already exists: {resolved_run_id}"
            )

        now = utc_now()
        last_absorbed = state["last_absorbed_upstream_commit"]
        candidate_commit_range = (
            f"{last_absorbed}..{upstream_head}"
            if last_absorbed
            else f"{merge_base}..{upstream_head}"
        )

        directories = [
            run_root / "commits",
            run_root / "reports",
            run_root / "decisions",
            run_root / "debates",
            run_root / "agents",
            run_root / "final",
        ]
        for directory in directories:
            directory.mkdir(parents=True, exist_ok=False)

        session = {
            "schema_version": SCHEMA_VERSION,
            "run_id": resolved_run_id,
            "status": "active",
            "phase": "init-run",
            "base_branch": base_branch,
            "sync_branch": sync_branch,
            "upstream_remote": upstream_remote,
            "upstream_branch": upstream_branch,
            "base_head_at_start": base_head or rev_parse(base_branch, repo_root=self.repo_root),
            "upstream_head_at_start": upstream_head,
            "merge_base_at_start": merge_base,
            "started_from_branch": started_from_branch or current_branch(self.repo_root),
            "candidate_commit_range": candidate_commit_range,
            "created_at": now,
            "updated_at": now,
        }
        run_index = {
            "schema_version": SCHEMA_VERSION,
            "run_id": resolved_run_id,
            "commit_ids": [],
            "report_ids": [],
            "decision_ids": [],
            "debate_ids": [],
            "assigned_agents": [],
            "blocking_items": [],
            "targets": {},
            "report_to_commit": {},
            "decision_to_target": {},
            "debate_to_target": {},
            "created_at": now,
            "updated_at": now,
        }
        reports_index = {
            "schema_version": SCHEMA_VERSION,
            "reports": [],
            "report_to_commit": {},
            "report_to_decision_ids": {},
            "report_to_debate_ids": {},
            "created_at": now,
            "updated_at": now,
        }
        write_json5_like(run_root / "session.json5", session)
        write_json5_like(run_root / "index.json5", run_index)
        write_json5_like(run_root / "reports" / "index.json5", reports_index)

        state["active_run_id"] = resolved_run_id
        state["last_upstream_head"] = upstream_head
        state["last_sync_branch"] = sync_branch
        self.save_state(state)

        return {
            "run_id": resolved_run_id,
            "session_path": str(
                (run_root / "session.json5").relative_to(self.repo_root)
            ),
            "state_updated": True,
        }

    def get_active_run(self) -> dict[str, Any]:
        state = self.load_state()
        run_id = state["active_run_id"]
        if run_id is None:
            return {"active_run_id": None}
        session = self.load_run_file(run_id, "session.json5")
        return {"active_run_id": run_id, "session": session}

    def load_run_index(self, run_id: str) -> dict[str, Any]:
        payload = self.load_run_file(run_id, "index.json5")
        payload.setdefault("targets", {})
        payload.setdefault("report_to_commit", {})
        payload.setdefault("decision_to_target", {})
        payload.setdefault("debate_to_target", {})
        payload.setdefault("auto_resolution_index", {})
        return payload

    def save_run_index(self, run_id: str, payload: dict[str, Any]) -> None:
        self.save_run_file(run_id, "index.json5", payload)

    def _next_id(self, prefix: str, ids: list[str]) -> str:
        return f"{prefix}-{len(ids) + 1:04d}"

    def _path_exists(self, run_id: str, relative_path: str) -> bool:
        return self.run_file(run_id, relative_path).exists()

    def load_reports_index(self, run_id: str) -> dict[str, Any]:
        payload = self.load_run_file(run_id, "reports/index.json5")
        payload.setdefault("report_to_commit", {})
        payload.setdefault("report_to_decision_ids", {})
        payload.setdefault("report_to_debate_ids", {})
        return payload

    def save_reports_index(self, run_id: str, payload: dict[str, Any]) -> None:
        self.save_run_file(run_id, "reports/index.json5", payload)

    def load_report(self, run_id: str, report_id: str) -> dict[str, Any]:
        reports_index = self.load_reports_index(run_id)
        for report in reports_index["reports"]:
            if report["report_id"] == report_id:
                report.setdefault("decision_ids", [])
                report.setdefault("debate_ids", [])
                return report
        raise SyncflowError("report_not_found", f"Unknown report id: {report_id}")

    def load_decision(self, run_id: str, decision_id: str) -> dict[str, Any]:
        path = self.run_file(run_id, f"decisions/{decision_id}.json5")
        if not path.exists():
            raise SyncflowError(
                "decision_not_found", f"Unknown decision id: {decision_id}"
            )
        payload = read_json5_like(path)
        payload.setdefault("related_commit_id", None)
        payload.setdefault("related_report_id", None)
        payload.setdefault("consensus_status", None)
        payload.setdefault("applied_at", None)
        payload.setdefault("applied_by", None)
        payload.setdefault("applied_summary", None)
        payload.setdefault("applied_paths", [])
        payload.setdefault("git_commit_sha", None)
        return payload

    def save_decision(self, run_id: str, payload: dict[str, Any]) -> None:
        self.save_run_file(run_id, f"decisions/{payload['decision_id']}.json5", payload)

    def resolve_target(
        self, run_id: str, target_type: str, target_id: str
    ) -> dict[str, str]:
        if target_type == "commit":
            self.load_commit(run_id, target_id)
            return {"target_type": target_type, "target_id": target_id}
        if target_type == "report":
            report = self.load_report(run_id, target_id)
            if not self._path_exists(run_id, f"reports/{target_id}.md"):
                raise SyncflowError(
                    "report_body_missing", f"Missing report body for: {target_id}"
                )
            return {"target_type": target_type, "target_id": report["report_id"]}
        if target_type == "decision":
            decision = self.load_decision(run_id, target_id)
            return {"target_type": target_type, "target_id": decision["decision_id"]}
        if target_type == "topic":
            if not target_id.strip():
                raise SyncflowError(
                    "invalid_target", "Topic target_id must not be empty"
                )
            return {"target_type": target_type, "target_id": target_id}
        raise SyncflowError(
            "invalid_target_type", f"Unsupported target_type: {target_type}"
        )

    def infer_target_type(self, run_id: str, target_id: str) -> str:
        if target_id.startswith("CMP-"):
            self.load_commit(run_id, target_id)
            return "commit"
        if target_id.startswith("RPT-"):
            self.load_report(run_id, target_id)
            return "report"
        if target_id.startswith("DEC-"):
            self.load_decision(run_id, target_id)
            return "decision"
        raise SyncflowError(
            "target_not_found", f"Cannot infer target type for: {target_id}"
        )

    def auto_domains_for_paths(self, paths: list[str]) -> list[str]:
        domain_order = [
            "workflow",
            "docs",
            "python",
            "rust",
            "ui",
            "extensions",
            "config",
            "infra",
            "legal",
            "misc",
        ]
        detected: set[str] = set()
        for path in paths:
            if path.startswith(".github/workflows/") or "workflow" in path:
                detected.add("workflow")
            elif path.endswith(".md") or path.startswith("docs/"):
                detected.add("docs")
            elif path.endswith(".py") or path.startswith("script/"):
                detected.add("python")
            elif path.endswith(".rs") or path.startswith("crates/"):
                detected.add("rust")
            elif path.startswith(("assets/", "crates/ui/", "crates/gpui/")):
                detected.add("ui")
            elif path.startswith("extensions/"):
                detected.add("extensions")
            elif path.endswith((".json", ".json5", ".toml", ".yaml", ".yml")):
                detected.add("config")
            elif path.startswith(("docker", "nix/", "flake", "compose")):
                detected.add("infra")
            elif path.startswith(("legal/", "LICENSE", "CODE_OF_CONDUCT", "CONTRIBUTING")):
                detected.add("legal")
            else:
                detected.add("misc")
        return [domain for domain in domain_order if domain in detected]

    def auto_risk_profile(self, paths: list[str]) -> dict[str, Any]:
        risk_tags: list[str] = []
        risk_level = "low"
        for path in paths:
            if path.startswith((".github/workflows/", "tooling/xtask/src/tasks/workflows/")):
                risk_tags.append("workflow-generated")
            if path.startswith(("README", "docs/", "legal/")):
                risk_tags.append("docs-policy")
            if "telemetry" in path.lower():
                risk_tags.append("telemetry")
            if any(keyword in path.lower() for keyword in ["auth", "provider", "privacy", "license"]):
                risk_tags.append("policy-sensitive")
            if path.startswith(("crates/", "extensions/")):
                risk_tags.append("runtime-surface")

        unique_tags = list(dict.fromkeys(risk_tags))
        if any(tag in unique_tags for tag in ["telemetry", "policy-sensitive"]):
            risk_level = "high"
        elif any(tag in unique_tags for tag in ["runtime-surface", "workflow-generated", "docs-policy"]):
            risk_level = "medium"

        return {
            "risk_tags": unique_tags,
            "risk_level": risk_level,
        }

    def filter_candidates(
        self,
        *,
        candidates: list[dict[str, object]],
        exclude_paths: list[str],
        include_domains: list[str],
    ) -> list[dict[str, object]]:
        filtered: list[dict[str, object]] = []
        domain_filter = set(include_domains)
        for candidate in candidates:
            paths_raw = candidate.get("paths", [])
            if not isinstance(paths_raw, list):
                continue
            paths = [str(path) for path in paths_raw]
            kept_paths = [
                path for path in paths if not any(excluded in path for excluded in exclude_paths)
            ]
            if not kept_paths and paths:
                continue
            domains = self.auto_domains_for_paths(kept_paths)
            if domain_filter and not domain_filter.intersection(domains):
                continue
            filtered.append({**candidate, "paths": kept_paths, "path_summary": kept_paths[:10]})
        return filtered

    def intake_candidates(
        self,
        *,
        run_id: str,
        candidates: list[dict[str, object]],
        replace_existing: bool = False,
    ) -> dict[str, Any]:
        self.require_active_run(run_id)
        self.maybe_advance_phase(run_id, "intake")
        index = self.load_run_index(run_id)
        if index["commit_ids"] and not replace_existing:
            raise SyncflowError(
                "commit_records_exist",
                "Commit records already exist for run. Pass replace_existing to rebuild intake.",
            )
        if replace_existing:
            for commit_id in list(index["commit_ids"]):
                commit_path = self.run_file(run_id, f"commits/{commit_id}.json5")
                if commit_path.exists():
                    commit_path.unlink()
            index["commit_ids"] = []
            index["blocking_items"] = [
                item
                for item in index["blocking_items"]
                if not str(item).startswith("CMP-")
            ]
            self.save_run_index(run_id, index)

        created_records: list[dict[str, Any]] = []
        for candidate in candidates:
            upstream_commit = str(candidate["sha"])
            title = str(candidate["title"])
            paths_raw = candidate.get("paths", [])
            if not isinstance(paths_raw, list):
                raise SyncflowError(
                    "invalid_candidate", "Candidate paths must be a list"
                )
            paths = [str(path) for path in paths_raw]
            domains = self.auto_domains_for_paths(paths)
            risk_profile = self.auto_risk_profile(paths)
            created = self.create_commit_record(
                run_id=run_id,
                upstream_commit=upstream_commit,
                title=title,
                domains=domains,
                risk_paths=paths[:20],
                path_summary=paths[:10],
                risk_tags=list(risk_profile["risk_tags"]),
                risk_level=str(risk_profile["risk_level"]),
            )
            created_records.append(
                {
                    "commit_id": created["commit_id"],
                    "upstream_commit": upstream_commit,
                    "title": title,
                    "domains": domains,
                    "risk_paths": paths[:20],
                    "risk_tags": risk_profile["risk_tags"],
                    "risk_level": risk_profile["risk_level"],
                }
            )

        return {
            "run_id": run_id,
            "created_count": len(created_records),
            "created_records": created_records,
        }

    def create_commit_record(
        self,
        *,
        run_id: str,
        upstream_commit: str,
        title: str,
        domains: list[str],
        risk_paths: list[str],
        path_summary: list[str] | None = None,
        risk_tags: list[str] | None = None,
        risk_level: str | None = None,
    ) -> dict[str, Any]:
        self.require_active_run(run_id)
        self.maybe_advance_phase(run_id, "intake")
        index = self.load_run_index(run_id)
        commit_id = self._next_id("CMP", index["commit_ids"])
        payload = {
            "schema_version": SCHEMA_VERSION,
            "commit_id": commit_id,
            "upstream_commit": upstream_commit,
            "title": title,
            "domains": domains,
            "risk_paths": risk_paths,
            "path_summary": path_summary or risk_paths[:10],
            "risk_tags": risk_tags or [],
            "risk_level": risk_level or "low",
            "score": None,
            "review_status": "pending",
            "report_ids": [],
            "decision_ids": [],
            "debate_ids": [],
            "created_at": utc_now(),
            "updated_at": utc_now(),
        }
        write_json5_like(self.run_file(run_id, f"commits/{commit_id}.json5"), payload)
        index["commit_ids"].append(commit_id)
        index["targets"][commit_id] = {
            "target_type": "commit",
            "commit_id": commit_id,
            "report_ids": [],
            "decision_ids": [],
            "debate_ids": [],
        }
        self.save_run_index(run_id, index)
        return {
            "commit_id": commit_id,
            "path": f".sync/runs/{run_id}/commits/{commit_id}.json5",
        }

    def load_commit(self, run_id: str, commit_id: str) -> dict[str, Any]:
        path = self.run_file(run_id, f"commits/{commit_id}.json5")
        if not path.exists():
            raise SyncflowError("commit_not_found", f"Unknown commit id: {commit_id}")
        payload = read_json5_like(path)
        payload.setdefault("path_summary", payload.get("risk_paths", [])[:10])
        payload.setdefault("risk_tags", [])
        payload.setdefault("risk_level", "low")
        payload.setdefault("auto_resolution", None)
        return payload

    def controlled_auto_route_for_commit(self, commit: dict[str, Any]) -> dict[str, Any]:
        score = commit.get("score")
        if not isinstance(score, dict):
            return {
                "status": "manual",
                "eligible": False,
                "agent": None,
                "reasons": ["score-missing"],
                "fallback": "score-first",
            }

        breakdown = score.get("breakdown") or score_breakdown(score)
        domains = commit.get("domains", [])
        domain_count = len(domains)
        reasons: list[str] = []

        if score.get("rating") != "safe":
            reasons.append(f"rating-{score.get('rating', 'unknown')}")
        if commit.get("risk_level") == "high":
            reasons.append("risk-level-high")
        if domain_count != 1:
            reasons.append("multi-domain")
        if score.get("confidence", 0) <= 1:
            reasons.append("confidence-low")
        if score.get("merge_risk", 0) >= 2:
            reasons.append("merge-risk-not-low")
        if score.get("behavior_regression_risk", 0) >= 2:
            reasons.append("behavior-risk-not-low")
        if score.get("maintenance_cost", 0) >= 2:
            reasons.append("maintenance-cost-not-low")
        if score.get("upstream_alignment", 0) <= 1:
            reasons.append("upstream-alignment-low")

        risk_tags = set(commit.get("risk_tags", []))
        if "policy-sensitive" in risk_tags:
            reasons.append("policy-sensitive")
        if "telemetry" in risk_tags:
            reasons.append("telemetry")
        if "runtime-surface" in risk_tags:
            reasons.append("runtime-surface")

        domain = domains[0] if domains else "misc"
        selected_agent: str | None = None
        if domain == "docs":
            selected_agent = "docs-policy-agent"
        elif domain == "workflow":
            selected_agent = "workflow-merge-agent"
        elif domain == "rust":
            selected_agent = "rust-merge-agent"
        elif domain == "python":
            selected_agent = "workflow-merge-agent"

        if selected_agent is None:
            reasons.append(f"unsupported-domain:{domain}")

        if domain == "docs" and "docs-policy" not in risk_tags:
            reasons.append("docs-signal-missing")
        if domain == "workflow" and "workflow-generated" not in risk_tags:
            reasons.append("workflow-signal-missing")

        eligible = not reasons and selected_agent is not None
        return {
            "status": "eligible" if eligible else "manual",
            "eligible": eligible,
            "agent": selected_agent,
            "domain": domain,
            "reasons": reasons,
            "fallback": "human-decision-with-challenger-review",
            "score_breakdown": breakdown,
        }

    def save_commit(self, run_id: str, payload: dict[str, Any]) -> None:
        self.save_run_file(run_id, f"commits/{payload['commit_id']}.json5", payload)

    def score_commit(
        self, *, run_id: str, commit_id: str, agent: str, score: dict[str, int]
    ) -> dict[str, Any]:
        self.require_active_run(run_id)
        self.maybe_advance_phase(run_id, "score")
        commit = self.load_commit(run_id, commit_id)
        for field_name, value in score.items():
            if value < 0 or value > 4:
                raise SyncflowError(
                    "invalid_score", f"Score field out of range: {field_name}={value}"
                )

        rating = commit_rating(score)
        breakdown = score_breakdown(score)
        commit["score"] = {
            **score,
            "rating": rating,
            "agent": agent,
            "breakdown": breakdown,
        }
        commit["review_status"] = "blocked" if rating == "block" else "scored"
        commit["auto_resolution"] = self.controlled_auto_route_for_commit(commit)
        self.save_commit(run_id, commit)

        index = self.load_run_index(run_id)
        if rating == "block" and commit_id not in index["blocking_items"]:
            index["blocking_items"].append(commit_id)
        if rating != "block" and commit_id in index["blocking_items"]:
            index["blocking_items"].remove(commit_id)
        if agent not in index["assigned_agents"]:
            index["assigned_agents"].append(agent)
        self.save_run_index(run_id, index)

        return {
            "commit_id": commit_id,
            "score": commit["score"],
            "review_status": commit["review_status"],
            "needs_challenger_review": needs_challenger_review(rating, score),
            "auto_resolution": commit["auto_resolution"],
        }

    def assign_domain(
        self,
        *,
        run_id: str,
        commit_id: str,
        domains: list[str],
    ) -> dict[str, Any]:
        self.require_active_run(run_id)
        self.maybe_advance_phase(run_id, "classify")
        commit = self.load_commit(run_id, commit_id)
        commit["domains"] = list(dict.fromkeys(domains))
        if commit.get("score") is not None:
            commit["auto_resolution"] = self.controlled_auto_route_for_commit(commit)
        self.save_commit(run_id, commit)
        return {
            "run_id": run_id,
            "commit_id": commit_id,
            "domains": commit["domains"],
        }

    def write_agent_result(
        self,
        *,
        run_id: str,
        agent: str,
        target_type: str,
        target_id: str,
        status: str,
        summary: str,
        payload: dict[str, Any],
    ) -> dict[str, Any]:
        self.require_active_run(run_id)
        resolved_target = self.resolve_target(run_id, target_type, target_id)
        shard_path = self.run_file(run_id, f"agents/{agent}.json5")
        if shard_path.exists():
            shard = read_json5_like(shard_path)
        else:
            shard = {
                "schema_version": SCHEMA_VERSION,
                "agent": agent,
                "results": [],
                "created_at": utc_now(),
                "updated_at": utc_now(),
            }
        results = list(shard.get("results", []))
        results.append(
            {
                "target_type": resolved_target["target_type"],
                "target_id": resolved_target["target_id"],
                "status": status,
                "summary": summary,
                "payload": payload,
                "recorded_at": utc_now(),
            }
        )
        shard["results"] = results
        write_json5_like(shard_path, shard)
        index = self.load_run_index(run_id)
        if agent not in index["assigned_agents"]:
            index["assigned_agents"].append(agent)
            self.save_run_index(run_id, index)
        return {
            "run_id": run_id,
            "agent": agent,
            "target_type": resolved_target["target_type"],
            "target_id": resolved_target["target_id"],
            "result_count": len(results),
            "path": f".sync/runs/{run_id}/agents/{agent}.json5",
        }

    def list_commits(
        self,
        *,
        run_id: str,
        review_status: str | None = None,
        domain: str | None = None,
        risk_level: str | None = None,
        auto_only: bool = False,
    ) -> dict[str, Any]:
        index = self.load_run_index(run_id)
        commits: list[dict[str, Any]] = []
        for commit_id in index.get("commit_ids", []):
            commit = self.load_commit(run_id, commit_id)
            auto_resolution = commit.get("auto_resolution") or self.controlled_auto_route_for_commit(commit)
            if review_status is not None:
                require_enum("review_status", review_status, REVIEW_STATUSES)
                if commit.get("review_status") != review_status:
                    continue
            if domain is not None and domain not in commit.get("domains", []):
                continue
            if risk_level is not None and commit.get("risk_level") != risk_level:
                continue
            if auto_only and not auto_resolution.get("eligible"):
                continue
            commits.append(
                {
                    "commit_id": commit["commit_id"],
                    "title": commit["title"],
                    "domains": commit.get("domains", []),
                    "risk_level": commit.get("risk_level", "low"),
                    "risk_tags": commit.get("risk_tags", []),
                    "review_status": commit.get("review_status"),
                    "score": commit.get("score"),
                    "auto_resolution": auto_resolution,
                }
            )
        return {"run_id": run_id, "commits": commits}

    def auto_resolve_candidates(self, *, run_id: str) -> dict[str, Any]:
        self.require_active_run(run_id)
        self.maybe_advance_phase(run_id, "auto-resolve")
        candidates = self.list_commits(run_id=run_id, auto_only=False)["commits"]
        return {
            "run_id": run_id,
            "candidates": candidates,
            "eligible_commit_ids": [
                commit["commit_id"]
                for commit in candidates
                if commit["auto_resolution"].get("eligible")
            ],
        }

    def auto_resolve_commit(
        self,
        *,
        run_id: str,
        commit_id: str,
        agent: str,
        summary: str,
        chosen_action: str,
    ) -> dict[str, Any]:
        self.require_active_run(run_id)
        self.maybe_advance_phase(run_id, "auto-resolve")
        require_enum("chosen_action", chosen_action, DECISION_ACTIONS)
        commit = self.load_commit(run_id, commit_id)
        route = commit.get("auto_resolution") or self.controlled_auto_route_for_commit(commit)
        if not route.get("eligible"):
            raise SyncflowError(
                "auto_resolve_not_allowed",
                f"Commit {commit_id} requires escalation: {route.get('reasons', [])}",
            )
        expected_agent = route.get("agent")
        if agent != expected_agent:
            raise SyncflowError(
                "auto_resolve_agent_mismatch",
                f"Commit {commit_id} must use {expected_agent}, got {agent}",
            )

        commit["review_status"] = "auto-resolved"
        commit["auto_resolution"] = {
            **route,
            "status": "resolved",
            "resolved_at": utc_now(),
            "resolved_by": agent,
            "chosen_action": chosen_action,
            "summary": summary,
        }
        self.save_commit(run_id, commit)

        index = self.load_run_index(run_id)
        index.setdefault("auto_resolution_index", {})[commit_id] = {
            "status": "resolved",
            "agent": agent,
            "chosen_action": chosen_action,
            "summary": summary,
        }
        if agent not in index["assigned_agents"]:
            index["assigned_agents"].append(agent)
        self.save_run_index(run_id, index)
        return {
            "run_id": run_id,
            "commit_id": commit_id,
            "review_status": commit["review_status"],
            "auto_resolution": commit["auto_resolution"],
        }

    def create_report(
        self, *, run_id: str, commit_id: str, title: str
    ) -> dict[str, Any]:
        self.require_active_run(run_id)
        self.maybe_advance_phase(run_id, "draft-reports")
        commit = self.load_commit(run_id, commit_id)
        index = self.load_run_index(run_id)
        report_id = self._next_id("RPT", index["report_ids"])
        report_path = self.run_file(run_id, f"reports/{report_id}.md")
        report_rel_path = f".sync/runs/{run_id}/reports/{report_id}.md"
        score = commit.get("score") or {}
        score_rating = score.get("rating", "unknown")
        recommendation = (
            "Requires challenger review and human decision."
            if commit.get("risk_level") == "high" or score_rating in {"block", "needs-review"}
            else "Proceed with standard human decision review."
        )
        report_path.write_text(
            "\n".join(
                [
                    f"# {title}",
                    "",
                    f"- Report ID: {report_id}",
                    f"- Commit ID: {commit_id}",
                    "- Status: draft",
                    f"- Review Status: {commit.get('review_status', 'pending')}",
                    f"- Risk Level: {commit.get('risk_level', 'low')}",
                    f"- Score Rating: {score_rating}",
                    "",
                    "## Background",
                    "",
                    f"Upstream commit {commit_id} targets: {commit.get('title', '')}",
                    f"Domains: {', '.join(commit.get('domains', [])) or 'none'}",
                    f"Risk paths: {', '.join(commit.get('risk_paths', [])) or 'none'}",
                    "",
                    "## Evidence",
                    "",
                    f"Path summary: {', '.join(commit.get('path_summary', [])) or 'none'}",
                    f"Risk tags: {', '.join(commit.get('risk_tags', [])) or 'none'}",
                    "",
                    "## Risk",
                    "",
                    f"Current rating: {score_rating}",
                    f"Risk profile: {commit.get('risk_level', 'low')}",
                    "",
                    "## Project Philosophy Relation",
                    "",
                    "Assess whether the diff strengthens ZZZ's default behavior, privacy, governance, and policy boundaries.",
                    "",
                    "## Current Recommendation",
                    "",
                    recommendation,
                    "",
                    "## Related IDs",
                    "",
                    f"- Report: {report_id}",
                    f"- Commit: {commit_id}",
                    f"- Decisions: {', '.join(commit.get('decision_ids', [])) or 'none'}",
                    f"- Debates: {', '.join(commit.get('debate_ids', [])) or 'none'}",
                    "",
                ]
            ),
            encoding="utf-8",
        )

        reports_index = self.load_reports_index(run_id)
        reports_index["reports"].append(
            {
                "report_id": report_id,
                "title": title,
                "commit_id": commit_id,
                "status": "active",
                "path": report_rel_path,
                "decision_ids": [],
                "debate_ids": [],
            }
        )
        reports_index["report_to_commit"][report_id] = commit_id
        reports_index["report_to_decision_ids"][report_id] = []
        reports_index["report_to_debate_ids"][report_id] = []
        self.save_reports_index(run_id, reports_index)
        index["report_ids"].append(report_id)
        index["report_to_commit"][report_id] = commit_id
        commit_target = index["targets"].setdefault(
            commit_id,
            {
                "target_type": "commit",
                "commit_id": commit_id,
                "report_ids": [],
                "decision_ids": [],
                "debate_ids": [],
            },
        )
        if report_id not in commit_target["report_ids"]:
            commit_target["report_ids"].append(report_id)
        index["targets"][report_id] = {
            "target_type": "report",
            "commit_id": commit_id,
            "report_id": report_id,
            "decision_ids": [],
            "debate_ids": [],
        }
        self.save_run_index(run_id, index)
        commit["report_ids"].append(report_id)
        if commit["review_status"] == "scored":
            commit["review_status"] = "needs-report"
        self.save_commit(run_id, commit)

        return {"report_id": report_id, "path": report_rel_path}

    def get_report_by_id(self, *, run_id: str, report_id: str) -> dict[str, Any]:
        report = self.load_report(run_id, report_id)
        body_path = self.run_file(run_id, f"reports/{report_id}.md")
        return {
            "run_id": run_id,
            "report_id": report_id,
            "title": report["title"],
            "commit_id": report["commit_id"],
            "status": report["status"],
            "path": report["path"],
            "decision_ids": report.get("decision_ids", []),
            "debate_ids": report.get("debate_ids", []),
            "body_path": f".sync/runs/{run_id}/reports/{report_id}.md",
            "body": body_path.read_text(encoding="utf-8"),
        }

    def update_report(
        self,
        *,
        run_id: str,
        report_id: str,
        title: str | None = None,
        body: str | None = None,
        status: str | None = None,
    ) -> dict[str, Any]:
        self.require_active_run(run_id)
        report = self.load_report(run_id, report_id)
        if title is not None:
            report["title"] = title
        if status is not None:
            require_enum("status", status, REPORT_STATUSES)
            report["status"] = status
        if body is not None:
            body_path = self.run_file(run_id, f"reports/{report_id}.md")
            body_path.write_text(body.rstrip() + "\n", encoding="utf-8")

        reports_index = self.load_reports_index(run_id)
        for indexed_report in reports_index["reports"]:
            if indexed_report["report_id"] != report_id:
                continue
            indexed_report["title"] = report["title"]
            indexed_report["status"] = report["status"]
            break
        self.save_reports_index(run_id, reports_index)

        index = self.load_run_index(run_id)
        if report_id in index["targets"]:
            index["targets"][report_id]["status"] = report["status"]
            self.save_run_index(run_id, index)
        return {
            "run_id": run_id,
            "report_id": report_id,
            "status": report["status"],
            "path": f".sync/runs/{run_id}/reports/{report_id}.md",
        }

    def update_report_index(
        self,
        *,
        run_id: str,
        report_id: str,
        decision_ids: list[str] | None = None,
        debate_ids: list[str] | None = None,
        status: str | None = None,
    ) -> dict[str, Any]:
        self.require_active_run(run_id)
        report = self.load_report(run_id, report_id)
        if status is not None:
            require_enum("status", status, REPORT_STATUSES)
            report["status"] = status
        if decision_ids is not None:
            for decision_id in decision_ids:
                self.load_decision(run_id, decision_id)
            report["decision_ids"] = list(dict.fromkeys(decision_ids))
        if debate_ids is not None:
            for debate_id in debate_ids:
                self.load_run_file(run_id, f"debates/{debate_id}.json5")
            report["debate_ids"] = list(dict.fromkeys(debate_ids))

        reports_index = self.load_reports_index(run_id)
        for indexed_report in reports_index["reports"]:
            if indexed_report["report_id"] != report_id:
                continue
            indexed_report["status"] = report["status"]
            indexed_report["decision_ids"] = report.get("decision_ids", [])
            indexed_report["debate_ids"] = report.get("debate_ids", [])
            break
        reports_index.setdefault("report_to_decision_ids", {})[report_id] = report.get(
            "decision_ids", []
        )
        reports_index.setdefault("report_to_debate_ids", {})[report_id] = report.get(
            "debate_ids", []
        )
        self.save_reports_index(run_id, reports_index)

        index = self.load_run_index(run_id)
        target = index["targets"].setdefault(
            report_id,
            {
                "target_type": "report",
                "report_id": report_id,
                "commit_id": report["commit_id"],
                "decision_ids": [],
                "debate_ids": [],
            },
        )
        target["status"] = report["status"]
        target["decision_ids"] = report.get("decision_ids", [])
        target["debate_ids"] = report.get("debate_ids", [])
        self.save_run_index(run_id, index)
        return {
            "run_id": run_id,
            "report_id": report_id,
            "status": report["status"],
            "decision_ids": report.get("decision_ids", []),
            "debate_ids": report.get("debate_ids", []),
        }

    def list_reports(self, *, run_id: str, status: str | None = None) -> dict[str, Any]:
        reports_index = self.load_reports_index(run_id)
        reports = reports_index.get("reports", [])
        if status is not None:
            require_enum("status", status, REPORT_STATUSES)
            reports = [report for report in reports if report.get("status") == status]
        return {
            "run_id": run_id,
            "status": status or "all",
            "reports": reports,
        }

    def archive_report(self, *, run_id: str, report_id: str) -> dict[str, Any]:
        self.require_active_run(run_id)
        return self.update_report(run_id=run_id, report_id=report_id, status="archived")

    def archive_all_reports(self, *, run_id: str) -> dict[str, Any]:
        reports_index = self.load_reports_index(run_id)
        archived_report_ids: list[str] = []
        for report in reports_index.get("reports", []):
            report_id = report["report_id"]
            if report.get("status") == "archived":
                continue
            self.update_report(run_id=run_id, report_id=report_id, status="archived")
            archived_report_ids.append(report_id)
        return {
            "run_id": run_id,
            "archived_report_ids": archived_report_ids,
        }

    def _attach_target_link(
        self, run_id: str, target_id: str, field_name: str, object_id: str
    ) -> None:
        if target_id.startswith("CMP-"):
            commit = self.load_commit(run_id, target_id)
            links = commit[field_name]
            if object_id not in links:
                links.append(object_id)
            self.save_commit(run_id, commit)
            index = self.load_run_index(run_id)
            target_entry = index["targets"].setdefault(
                target_id,
                {
                    "target_type": "commit",
                    "commit_id": target_id,
                    "report_ids": [],
                    "decision_ids": [],
                    "debate_ids": [],
                },
            )
            target_links = target_entry.setdefault(field_name, [])
            if object_id not in target_links:
                target_links.append(object_id)
            self.save_run_index(run_id, index)
            return

        if target_id.startswith("RPT-"):
            reports_index = self.load_reports_index(run_id)
            for report in reports_index["reports"]:
                if report["report_id"] != target_id:
                    continue
                links = report.setdefault(field_name, [])
                if object_id not in links:
                    links.append(object_id)
                mapping_key = (
                    "report_to_decision_ids"
                    if field_name == "decision_ids"
                    else "report_to_debate_ids"
                )
                mapping = reports_index.setdefault(mapping_key, {})
                mapping.setdefault(target_id, [])
                if object_id not in mapping[target_id]:
                    mapping[target_id].append(object_id)
                self.save_reports_index(run_id, reports_index)

                index = self.load_run_index(run_id)
                target_entry = index["targets"].setdefault(
                    target_id,
                    {
                        "target_type": "report",
                        "report_id": target_id,
                        "commit_id": report["commit_id"],
                        "decision_ids": [],
                        "debate_ids": [],
                    },
                )
                target_links = target_entry.setdefault(field_name, [])
                if object_id not in target_links:
                    target_links.append(object_id)
                self.save_run_index(run_id, index)
                return

    def _attach_decision_to_target(
        self, run_id: str, target_type: str, target_id: str, decision_id: str
    ) -> None:
        if target_type == "commit":
            self._attach_target_link(run_id, target_id, "decision_ids", decision_id)
            index = self.load_run_index(run_id)
            index["decision_to_target"][decision_id] = {
                "target_type": target_type,
                "target_id": target_id,
                "related_commit_id": target_id,
                "related_report_id": None,
            }
            self.save_run_index(run_id, index)
            return
        if target_type == "report":
            report = self.load_report(run_id, target_id)
            commit = self.load_commit(run_id, report["commit_id"])
            if decision_id not in commit["decision_ids"]:
                commit["decision_ids"].append(decision_id)
            self.save_commit(run_id, commit)
            self._attach_target_link(run_id, report["commit_id"], "decision_ids", decision_id)
            self._attach_target_link(run_id, target_id, "decision_ids", decision_id)
            index = self.load_run_index(run_id)
            index["decision_to_target"][decision_id] = {
                "target_type": target_type,
                "target_id": target_id,
                "related_commit_id": report["commit_id"],
                "related_report_id": target_id,
            }
            self.save_run_index(run_id, index)

    def record_decision(
        self,
        *,
        run_id: str,
        target_type: str,
        target_id: str,
        chosen_action: str,
        rationale_summary: str,
        human_feedback: str,
    ) -> dict[str, Any]:
        self.require_active_run(run_id)
        self.maybe_advance_phase(run_id, "human-decision")
        require_enum("target_type", target_type, DECISION_TARGET_TYPES)
        require_enum("chosen_action", chosen_action, DECISION_ACTIONS)
        resolved_target = self.resolve_target(run_id, target_type, target_id)

        index = self.load_run_index(run_id)
        decision_id = self._next_id("DEC", index["decision_ids"])
        payload = {
            "schema_version": SCHEMA_VERSION,
            "decision_id": decision_id,
            "target_type": resolved_target["target_type"],
            "target_id": resolved_target["target_id"],
            "related_commit_id": None,
            "related_report_id": None,
            "chosen_action": chosen_action,
            "rationale_summary": rationale_summary,
            "human_feedback": human_feedback,
            "debate_status": "pending",
            "resolution_status": "open",
            "consensus_status": None,
            "applied_at": None,
            "applied_by": None,
            "applied_summary": None,
            "applied_paths": [],
            "git_commit_sha": None,
            "created_at": utc_now(),
            "updated_at": utc_now(),
        }
        write_json5_like(
            self.run_file(run_id, f"decisions/{decision_id}.json5"), payload
        )
        index["decision_ids"].append(decision_id)
        index["targets"][decision_id] = {
            "target_type": "decision",
            "decision_id": decision_id,
            "target_id": resolved_target["target_id"],
            "decision_status": "open",
            "debate_status": "pending",
        }
        self.save_run_index(run_id, index)
        self._attach_decision_to_target(
            run_id,
            resolved_target["target_type"],
            resolved_target["target_id"],
            decision_id,
        )
        linked = self.load_decision(run_id, decision_id)
        linked_target = self.load_run_index(run_id)["decision_to_target"][decision_id]
        linked["related_commit_id"] = linked_target["related_commit_id"]
        linked["related_report_id"] = linked_target["related_report_id"]
        self.save_decision(run_id, linked)
        return {"decision_id": decision_id, "debate_required": True}

    def record_debate(
        self,
        *,
        run_id: str,
        target_id: str,
        trigger_reason: str,
        recommendation: str,
        disagreement_level: str,
        challenger_agent: str,
    ) -> dict[str, Any]:
        self.require_active_run(run_id)
        self.maybe_advance_phase(run_id, "challenger-review")
        require_enum("trigger_reason", trigger_reason, DEBATE_TRIGGER_REASONS)
        require_enum("disagreement_level", disagreement_level, DISAGREEMENT_LEVELS)
        target_type = self.infer_target_type(run_id, target_id)

        index = self.load_run_index(run_id)
        debate_id = self._next_id("DEB", index["debate_ids"])
        payload = {
            "schema_version": SCHEMA_VERSION,
            "debate_id": debate_id,
            "target_type": target_type,
            "target_id": target_id,
            "trigger_reason": trigger_reason,
            "challenger_agent": challenger_agent,
            "status": "active",
            "disagreement_level": disagreement_level,
            "recommendation": recommendation,
            "created_at": utc_now(),
            "updated_at": utc_now(),
        }
        write_json5_like(self.run_file(run_id, f"debates/{debate_id}.json5"), payload)
        index["debate_ids"].append(debate_id)
        index["targets"][debate_id] = {
            "target_type": "debate",
            "debate_id": debate_id,
            "target_id": target_id,
            "status": "active",
        }
        if challenger_agent not in index["assigned_agents"]:
            index["assigned_agents"].append(challenger_agent)
        index["debate_to_target"][debate_id] = {
            "target_type": target_type,
            "target_id": target_id,
        }
        self.save_run_index(run_id, index)
        if target_type == "commit":
            self._attach_target_link(run_id, target_id, "debate_ids", debate_id)
        elif target_type == "decision":
            decision = self.load_decision(run_id, target_id)
            decision["debate_status"] = "active"
            self.save_decision(run_id, decision)
        elif target_type == "report":
            report = self.load_report(run_id, target_id)
            self._attach_target_link(run_id, report["commit_id"], "debate_ids", debate_id)
            self._attach_target_link(run_id, target_id, "debate_ids", debate_id)
        return {"debate_id": debate_id, "status": payload["status"]}

    def advance_phase(self, *, run_id: str, target_phase: str) -> dict[str, Any]:
        session = self.require_active_run(run_id)
        require_enum("target_phase", target_phase, RUN_PHASES)
        current_index = phase_index(session["phase"])
        target_index = phase_index(target_phase)
        if target_index < current_index:
            raise SyncflowError("phase_regression", f"Cannot regress phase from {session['phase']} to {target_phase}")
        if target_index > current_index + 1:
            raise SyncflowError("phase_skip", f"Cannot skip phase from {session['phase']} to {target_phase}")
        session["phase"] = target_phase
        self.save_session(run_id, session)
        return {"run_id": run_id, "phase": target_phase}

    def mark_resolved(
        self,
        *,
        run_id: str,
        decision_id: str,
        resolution_status: str = "resolved",
    ) -> dict[str, Any]:
        self.require_active_run(run_id)
        require_enum("resolution_status", resolution_status, RESOLUTION_STATUSES)
        decision = self.load_decision(run_id, decision_id)
        decision["resolution_status"] = resolution_status
        self.save_decision(run_id, decision)
        index = self.load_run_index(run_id)
        if decision_id in index["targets"]:
            index["targets"][decision_id]["decision_status"] = resolution_status
            self.save_run_index(run_id, index)
        self.maybe_advance_phase(run_id, "apply-decisions")
        return {
            "run_id": run_id,
            "decision_id": decision_id,
            "resolution_status": decision["resolution_status"],
        }

    def apply_decision(
        self,
        *,
        run_id: str,
        decision_id: str,
        applied_by: str,
        applied_summary: str,
        applied_paths: list[str],
        git_commit_sha: str | None,
    ) -> dict[str, Any]:
        self.require_active_run(run_id)
        decision = self.load_decision(run_id, decision_id)
        if decision["debate_status"] != "completed":
            raise SyncflowError(
                "debate_incomplete",
                f"Cannot apply decision before challenger review completes: {decision_id}",
            )
        if git_commit_sha is not None and not ref_exists(git_commit_sha, repo_root=self.repo_root):
            raise SyncflowError("git_commit_not_found", f"Unknown git commit: {git_commit_sha}")
        decision["applied_at"] = utc_now()
        decision["applied_by"] = applied_by
        decision["applied_summary"] = applied_summary
        decision["applied_paths"] = applied_paths
        decision["git_commit_sha"] = git_commit_sha
        decision["resolution_status"] = "resolved"
        self.save_decision(run_id, decision)

        index = self.load_run_index(run_id)
        if decision_id in index["targets"]:
            index["targets"][decision_id]["decision_status"] = "resolved"
            index["targets"][decision_id]["applied_by"] = applied_by
            index["targets"][decision_id]["applied_paths"] = applied_paths
            index["targets"][decision_id]["git_commit_sha"] = git_commit_sha
            self.save_run_index(run_id, index)

        related_commit_id = decision.get("related_commit_id")
        if related_commit_id:
            commit = self.load_commit(run_id, str(related_commit_id))
            commit["review_status"] = "resolved"
            self.save_commit(run_id, commit)

        related_report_id = decision.get("related_report_id")
        if related_report_id:
            reports_index = self.load_reports_index(run_id)
            for report in reports_index["reports"]:
                if report["report_id"] != related_report_id:
                    continue
                report["status"] = "resolved"
                break
            self.save_reports_index(run_id, reports_index)
            index = self.load_run_index(run_id)
            if related_report_id in index["targets"]:
                index["targets"][related_report_id]["status"] = "resolved"
                self.save_run_index(run_id, index)

        self.maybe_advance_phase(run_id, "apply-decisions")
        return {
            "run_id": run_id,
            "decision_id": decision_id,
            "resolution_status": decision["resolution_status"],
            "applied_by": applied_by,
            "applied_paths": applied_paths,
            "git_commit_sha": git_commit_sha,
        }

    def consensus_review(
        self,
        *,
        run_id: str,
        decision_id: str,
        debate_id: str,
        disagreement_level: str,
        summary: str,
        escalate_to_human: bool,
    ) -> dict[str, Any]:
        self.require_active_run(run_id)
        require_enum("disagreement_level", disagreement_level, DISAGREEMENT_LEVELS)
        decision = self.load_decision(run_id, decision_id)
        debate = self.load_run_file(run_id, f"debates/{debate_id}.json5")
        if debate["target_id"] != decision_id:
            raise SyncflowError(
                "decision_debate_mismatch",
                f"Debate {debate_id} does not target decision {decision_id}",
            )
        debate["disagreement_level"] = disagreement_level
        debate["status"] = "escalated" if escalate_to_human else "completed"
        debate["consensus_summary"] = summary
        self.save_run_file(run_id, f"debates/{debate_id}.json5", debate)

        decision["consensus_status"] = disagreement_level
        decision["debate_status"] = "completed" if not escalate_to_human else "active"
        self.save_decision(run_id, decision)

        index = self.load_run_index(run_id)
        if debate_id in index["targets"]:
            index["targets"][debate_id]["status"] = debate["status"]
            index["targets"][debate_id]["disagreement_level"] = disagreement_level
        if decision_id in index["targets"]:
            index["targets"][decision_id]["debate_status"] = decision["debate_status"]
            index["targets"][decision_id]["consensus_status"] = disagreement_level
        self.save_run_index(run_id, index)

        self.maybe_advance_phase(run_id, "consensus-review")
        return {
            "run_id": run_id,
            "decision_id": decision_id,
            "debate_id": debate_id,
            "disagreement_level": disagreement_level,
            "debate_status": debate["status"],
            "escalate_to_human": escalate_to_human,
        }

    def check_run_consistency(self, *, run_id: str) -> dict[str, Any]:
        session = self.load_run_file(run_id, "session.json5")
        index = self.load_run_index(run_id)
        state = self.load_state()
        archive = self.load_archive_index()
        issues: list[str] = []
        resolved_decisions = 0
        active_debates = 0

        if session["status"] == "active" and state.get("active_run_id") != run_id:
            issues.append("active_run_state_mismatch")
        if session["status"] != "active" and state.get("active_run_id") == run_id:
            issues.append("closed_run_still_active")

        for decision_id in index.get("decision_ids", []):
            decision = self.load_decision(run_id, decision_id)
            if decision["resolution_status"] == "resolved":
                resolved_decisions += 1
            if decision["resolution_status"] != "resolved" and session["status"] == "completed":
                issues.append(f"completed_run_has_unresolved_decision:{decision_id}")
            if decision["debate_status"] != "completed" and session["status"] == "completed":
                issues.append(f"completed_run_has_incomplete_challenger_review:{decision_id}")

        for debate_id in index.get("debate_ids", []):
            debate = self.load_run_file(run_id, f"debates/{debate_id}.json5")
            if debate["status"] in {"pending", "active"}:
                active_debates += 1
            if debate["status"] != "completed" and session["status"] == "completed":
                issues.append(f"completed_run_has_open_debate:{debate_id}")
            if debate["disagreement_level"] == "high-disagreement" and session["status"] == "completed":
                issues.append(f"completed_run_has_high_disagreement:{debate_id}")

        for commit_id in index.get("blocking_items", []):
            issues.append(f"blocking_item_present:{commit_id}")

        summary_path = self.run_file(run_id, "final/summary.json5")
        if session["status"] in {"completed", "aborted", "failed"}:
            if not summary_path.exists():
                issues.append("completed_run_missing_final_summary")
            else:
                summary = read_json5_like(summary_path)
                if session["status"] == "completed" and summary.get("resolved_count") != resolved_decisions:
                    issues.append("resolved_count_mismatch")
                if summary.get("blocking_count") != len(index.get("blocking_items", [])):
                    issues.append("blocking_count_mismatch")
                if summary.get("sync_branch_deleted_local") and local_branch_exists(
                    session["sync_branch"], repo_root=self.repo_root
                ):
                    issues.append("sync_branch_expected_deleted_local")

        if session["status"] != "active":
            archive_run_ids = {str(run.get("run_id")) for run in archive.get("runs", [])}
            if run_id not in archive_run_ids:
                issues.append("closed_run_missing_archive_entry")

        return {
            "run_id": run_id,
            "status": session["status"],
            "phase": session["phase"],
            "resolved_decisions": resolved_decisions,
            "active_debates": active_debates,
            "blocking_items": list(index.get("blocking_items", [])),
            "issues": issues,
            "ok": not issues,
        }

    def get_run_summary(self, *, run_id: str) -> dict[str, Any]:
        session = self.load_run_file(run_id, "session.json5")
        index = self.load_run_index(run_id)
        reports_index = self.load_reports_index(run_id)
        consistency = self.check_run_consistency(run_id=run_id)
        return {
            "run_id": run_id,
            "status": session["status"],
            "phase": session["phase"],
            "base_branch": session["base_branch"],
            "sync_branch": session["sync_branch"],
            "candidate_commit_range": session["candidate_commit_range"],
            "counts": {
                "commits": len(index.get("commit_ids", [])),
                "reports": len(index.get("report_ids", [])),
                "reports_active": len([report for report in reports_index["reports"] if report.get("status") == "active"]),
                "reports_archived": len([report for report in reports_index["reports"] if report.get("status") == "archived"]),
                "decisions": len(index.get("decision_ids", [])),
                "debates": len(index.get("debate_ids", [])),
                "blocking_items": len(index.get("blocking_items", [])),
            },
            "consistency": consistency,
            "summary_path": f".sync/runs/{run_id}/final/summary.md",
        }

    def get_history_summary(self) -> dict[str, Any]:
        archive = self.load_archive_index()
        runs = archive.get("runs", [])
        status_counts: dict[str, int] = {}
        for run in runs:
            status = str(run.get("status", "unknown"))
            status_counts[status] = status_counts.get(status, 0) + 1
        return {
            "run_count": len(runs),
            "runs": runs,
            "status_counts": status_counts,
            "view_notes": {
                "active": "Use get-run-summary or list-reports --status active for current work.",
                "archive": "Use get-history-summary, list-decisions, or list-debates for archived evidence.",
            },
        }

    def list_decisions(
        self,
        *,
        run_id: str,
        resolution_status: str | None = None,
        target_type: str | None = None,
    ) -> dict[str, Any]:
        index = self.load_run_index(run_id)
        decisions: list[dict[str, Any]] = []
        if resolution_status is not None:
            require_enum("resolution_status", resolution_status, RESOLUTION_STATUSES)
        if target_type is not None and target_type != "decision":
            require_enum("target_type", target_type, DECISION_TARGET_TYPES)
        for decision_id in index.get("decision_ids", []):
            decision = self.load_decision(run_id, decision_id)
            if resolution_status is not None and decision.get("resolution_status") != resolution_status:
                continue
            if target_type is not None and decision.get("target_type") != target_type:
                continue
            decisions.append(decision)
        return {"run_id": run_id, "decisions": decisions}

    def list_debates(
        self,
        *,
        run_id: str,
        status: str | None = None,
        disagreement_level: str | None = None,
    ) -> dict[str, Any]:
        index = self.load_run_index(run_id)
        debates: list[dict[str, Any]] = []
        if status is not None:
            require_enum("status", status, DEBATE_STATUSES)
        if disagreement_level is not None:
            require_enum("disagreement_level", disagreement_level, DISAGREEMENT_LEVELS)
        for debate_id in index.get("debate_ids", []):
            debate = self.load_run_file(run_id, f"debates/{debate_id}.json5")
            if status is not None and debate.get("status") != status:
                continue
            if disagreement_level is not None and debate.get("disagreement_level") != disagreement_level:
                continue
            debates.append(debate)
        return {"run_id": run_id, "debates": debates}

    def complete_debate(
        self,
        *,
        run_id: str,
        debate_id: str,
        status: str,
        disagreement_level: str | None = None,
    ) -> dict[str, Any]:
        self.require_active_run(run_id)
        require_enum("status", status, DEBATE_STATUSES)
        debate = self.load_run_file(run_id, f"debates/{debate_id}.json5")
        debate["status"] = status
        if disagreement_level is not None:
            require_enum("disagreement_level", disagreement_level, DISAGREEMENT_LEVELS)
            debate["disagreement_level"] = disagreement_level
        self.save_run_file(run_id, f"debates/{debate_id}.json5", debate)
        index = self.load_run_index(run_id)
        if debate_id in index["targets"]:
            index["targets"][debate_id]["status"] = debate["status"]
            self.save_run_index(run_id, index)
        if debate["target_type"] == "decision":
            decision = self.load_decision(run_id, debate["target_id"])
            decision["debate_status"] = "completed" if status == "completed" else status
            self.save_decision(run_id, decision)
            index = self.load_run_index(run_id)
            if debate["target_id"] in index["targets"]:
                index["targets"][debate["target_id"]]["debate_status"] = decision["debate_status"]
                self.save_run_index(run_id, index)
        self.maybe_advance_phase(run_id, "consensus-review")
        return {
            "run_id": run_id,
            "debate_id": debate_id,
            "status": debate["status"],
            "disagreement_level": debate["disagreement_level"],
        }

    def abort_run(
        self,
        *,
        run_id: str,
        reason: str,
        delete_local_branch: bool,
        delete_remote_branch_flag: bool,
        remote_name: str,
    ) -> dict[str, Any]:
        session = self.require_active_run(run_id)
        index = self.load_run_index(run_id)
        sync_branch_deleted_local = False
        sync_branch_deleted_remote = False
        if delete_local_branch or delete_remote_branch_flag:
            cleanup_result = self.cleanup_sync_branch(
                run_id=run_id,
                delete_local=delete_local_branch,
                delete_remote=delete_remote_branch_flag,
                remote_name=remote_name,
            )
            sync_branch_deleted_local = cleanup_result["deleted_local"]
            sync_branch_deleted_remote = cleanup_result["deleted_remote"]

        self.archive_all_reports(run_id=run_id)
        session["phase"] = "finalize"
        session["status"] = "aborted"
        session["abort_reason"] = reason
        self.save_run_file(run_id, "session.json5", session)

        summary_rel_path = f".sync/runs/{run_id}/final/summary.md"
        write_json5_like(
            self.run_file(run_id, "final/summary.json5"),
            {
                "schema_version": SCHEMA_VERSION,
                "run_id": run_id,
                "outcome": "aborted",
                "abort_reason": reason,
                "merged_to_main": False,
                "sync_branch_deleted_local": sync_branch_deleted_local,
                "sync_branch_deleted_remote": sync_branch_deleted_remote,
                "report_count": len(index["report_ids"]),
                "resolved_count": 0,
                "blocking_count": len(index["blocking_items"]),
                "summary_path": summary_rel_path,
                "created_at": utc_now(),
                "updated_at": utc_now(),
            },
        )
        self.run_file(run_id, "final/summary.md").write_text(
            "\n".join(
                [
                    f"# Run {run_id} final summary",
                    "",
                    "- Outcome: aborted",
                    f"- Reason: {reason}",
                    f"- Deleted local sync branch: {sync_branch_deleted_local}",
                    f"- Deleted remote sync branch: {sync_branch_deleted_remote}",
                    "",
                ]
            ),
            encoding="utf-8",
        )

        archive = self.load_archive_index()
        archive["runs"].append(
            {
                "run_id": run_id,
                "status": "aborted",
                "started_at": session["created_at"],
                "completed_at": utc_now(),
                "upstream_head_at_start": session["upstream_head_at_start"],
                "absorbed_range": session["candidate_commit_range"],
                "report_count": len(index["report_ids"]),
                "active_report_count": 0,
                "archived_report_count": len(index["report_ids"]),
                "blocking_count": len(index["blocking_items"]),
                "summary_path": summary_rel_path,
            }
        )
        self.save_archive_index(archive)

        state = self.load_state()
        state["active_run_id"] = None
        self.save_state(state)
        return {
            "run_id": run_id,
            "outcome": "aborted",
            "reason": reason,
            "summary_path": summary_rel_path,
            "sync_branch_deleted_local": sync_branch_deleted_local,
            "sync_branch_deleted_remote": sync_branch_deleted_remote,
        }

    def cleanup_sync_branch(
        self,
        *,
        run_id: str,
        delete_local: bool,
        delete_remote: bool,
        remote_name: str,
    ) -> dict[str, Any]:
        session = self.load_run_file(run_id, "session.json5")
        sync_branch = session["sync_branch"]
        base_branch = session["base_branch"]
        if delete_local and current_branch(self.repo_root) == sync_branch:
            ensure_clean_worktree(self.repo_root, operation="cleanup-sync-branch")
            checkout(base_branch, repo_root=self.repo_root)
        deleted_local = delete_local_branch(sync_branch, repo_root=self.repo_root) if delete_local else False
        deleted_remote = delete_remote_branch(remote_name, sync_branch, repo_root=self.repo_root) if delete_remote else False
        return {
            "run_id": run_id,
            "sync_branch": sync_branch,
            "deleted_local": deleted_local,
            "deleted_remote": deleted_remote,
        }

    def finalize_run(
        self,
        *,
        run_id: str,
        outcome: str,
        merge_to_base: bool,
        delete_local_branch: bool,
        delete_remote_branch_flag: bool,
        remote_name: str,
    ) -> dict[str, Any]:
        require_enum("outcome", outcome, RUN_STATUSES - {"active"})
        session = self.require_active_run(run_id)
        index = self.load_run_index(run_id)
        unresolved_decisions = [
            decision_id
            for decision_id in index["decision_ids"]
            if self.load_decision(run_id, decision_id)["resolution_status"] != "resolved"
        ]
        incomplete_challenger_decisions = [
            decision_id
            for decision_id in index["decision_ids"]
            if self.load_decision(run_id, decision_id)["debate_status"] != "completed"
        ]
        incomplete_debates = [
            debate_id
            for debate_id in index["debate_ids"]
            if self.load_run_file(run_id, f"debates/{debate_id}.json5")["status"] != "completed"
        ]
        high_disagreement_debates = [
            debate_id
            for debate_id in index["debate_ids"]
            if self.load_run_file(run_id, f"debates/{debate_id}.json5")["disagreement_level"] == "high-disagreement"
        ]
        if index["blocking_items"]:
            raise SyncflowError("blocking_items_present", f"Cannot finalize run with blocking items: {index['blocking_items']}")
        if unresolved_decisions:
            raise SyncflowError("unresolved_decisions_present", f"Cannot finalize run with unresolved decisions: {unresolved_decisions}")
        if incomplete_challenger_decisions:
            raise SyncflowError("challenger_review_incomplete", f"Cannot finalize run before challenger review completes: {incomplete_challenger_decisions}")
        if incomplete_debates:
            raise SyncflowError("open_debates_present", f"Cannot finalize run with incomplete debates: {incomplete_debates}")
        if high_disagreement_debates:
            raise SyncflowError("high_disagreement_present", f"Cannot finalize run with high disagreement debates: {high_disagreement_debates}")

        merged_to_main = False
        sync_branch_deleted_local = False
        sync_branch_deleted_remote = False
        merge_commit_sha: str | None = None
        if merge_to_base:
            ensure_clean_worktree(self.repo_root, operation="finalize-run")
            merge_commit_sha = merge_no_ff(
                session["base_branch"],
                session["sync_branch"],
                repo_root=self.repo_root,
            )
            merged_to_main = True
        if delete_local_branch or delete_remote_branch_flag:
            cleanup_result = self.cleanup_sync_branch(
                run_id=run_id,
                delete_local=delete_local_branch,
                delete_remote=delete_remote_branch_flag,
                remote_name=remote_name,
            )
            sync_branch_deleted_local = cleanup_result["deleted_local"]
            sync_branch_deleted_remote = cleanup_result["deleted_remote"]

        self.archive_all_reports(run_id=run_id)

        session["phase"] = "finalize"
        session["status"] = outcome
        self.save_run_file(run_id, "session.json5", session)

        final_summary_path = self.run_file(run_id, "final/summary.json5")
        summary_rel_path = f".sync/runs/{run_id}/final/summary.md"
        summary = {
            "schema_version": SCHEMA_VERSION,
            "run_id": run_id,
            "outcome": outcome,
            "merged_to_main": merged_to_main,
            "sync_branch_deleted_local": sync_branch_deleted_local,
            "sync_branch_deleted_remote": sync_branch_deleted_remote,
            "report_count": len(index["report_ids"]),
            "resolved_count": len(index["decision_ids"]) - len(unresolved_decisions),
            "blocking_count": len(index["blocking_items"]),
            "summary_path": summary_rel_path,
            "created_at": utc_now(),
            "updated_at": utc_now(),
        }
        write_json5_like(final_summary_path, summary)
        self.run_file(run_id, "final/summary.md").write_text(
            "\n".join(
                [
                    f"# Run {run_id} final summary",
                    "",
                    f"- Outcome: {outcome}",
                    f"- Merged to base: {merged_to_main}",
                    f"- Deleted local sync branch: {sync_branch_deleted_local}",
                    f"- Deleted remote sync branch: {sync_branch_deleted_remote}",
                    f"- Merge commit: {merge_commit_sha}",
                    f"- Reports: {len(index['report_ids'])}",
                    f"- Decisions: {len(index['decision_ids'])}",
                    f"- Debates: {len(index['debate_ids'])}",
                    f"- Blocking items: {len(index['blocking_items'])}",
                    "",
                    f"- Base branch: {session['base_branch']}",
                    f"- Sync branch: {session['sync_branch']}",
                    "",
                ]
            ),
            encoding="utf-8",
        )

        reports_index = self.load_reports_index(run_id)
        archive = self.load_archive_index()
        archive["runs"].append(
            {
                "run_id": run_id,
                "status": outcome,
                "started_at": session["created_at"],
                "completed_at": utc_now(),
                "upstream_head_at_start": session["upstream_head_at_start"],
                "absorbed_range": session["candidate_commit_range"],
                "report_count": len(index["report_ids"]),
                "active_report_count": len(
                    [report for report in reports_index["reports"] if report.get("status") == "active"]
                ),
                "archived_report_count": len(
                    [report for report in reports_index["reports"] if report.get("status") == "archived"]
                ),
                "blocking_count": len(index["blocking_items"]),
                "summary_path": summary_rel_path,
            }
        )
        self.save_archive_index(archive)

        state = self.load_state()
        state["active_run_id"] = None
        state["last_completed_at"] = utc_now()
        state["last_absorbed_upstream_commit"] = session["upstream_head_at_start"]
        state["last_sync_merge_commit"] = merge_commit_sha
        self.save_state(state)

        return {
            "run_id": run_id,
            "outcome": outcome,
            "summary_path": summary_rel_path,
            "merged_to_main": merged_to_main,
            "sync_branch_deleted_local": sync_branch_deleted_local,
            "sync_branch_deleted_remote": sync_branch_deleted_remote,
            "merge_commit": merge_commit_sha,
        }
