from __future__ import annotations

from dataclasses import dataclass
from typing import Any

SCHEMA_VERSION = "v1"

RUN_STATUSES = {"active", "completed", "aborted", "failed"}
RUN_PHASES = {
    "preflight",
    "init-run",
    "intake",
    "classify",
    "score",
    "auto-resolve",
    "draft-reports",
    "human-decision",
    "challenger-review",
    "consensus-review",
    "apply-decisions",
    "finalize",
}
RUN_PHASE_SEQUENCE = [
    "preflight",
    "init-run",
    "intake",
    "classify",
    "score",
    "auto-resolve",
    "draft-reports",
    "human-decision",
    "challenger-review",
    "consensus-review",
    "apply-decisions",
    "finalize",
]
RATINGS = {"safe", "watch", "needs-review", "block"}
REVIEW_STATUSES = {
    "pending",
    "scored",
    "auto-resolved",
    "needs-report",
    "awaiting-human",
    "resolved",
    "blocked",
}
REPORT_STATUSES = {"active", "resolved", "archived"}
RESOLUTION_STATUSES = {"open", "in_progress", "resolved", "archived"}
DEBATE_STATUSES = {"pending", "active", "completed", "escalated"}
DISAGREEMENT_LEVELS = {
    "low",
    "medium",
    "high-disagreement",
    "mostly-resolved",
}
DECISION_TARGET_TYPES = {"commit", "report", "topic"}
DECISION_ACTIONS = {
    "accept-upstream",
    "keep-zzz",
    "manual-rework",
    "drop-upstream-change",
    "defer",
}
DEBATE_TRIGGER_REASONS = {
    "human-decision",
    "agent-request",
    "low-score",
    "policy-sensitive",
    "confidence-low",
}


class SyncflowError(Exception):
    def __init__(self, error_code: str, message: str) -> None:
        self.error_code = error_code
        self.message = message
        super().__init__(message)


@dataclass(slots=True)
class CommandResult:
    data: dict[str, Any]


def ok(data: dict[str, Any]) -> dict[str, Any]:
    return {"ok": True, "data": data}


def fail(error_code: str, message: str) -> dict[str, Any]:
    return {"ok": False, "error_code": error_code, "message": message}


def require_enum(name: str, value: str, allowed: set[str]) -> None:
    if value not in allowed:
        raise SyncflowError(
            "invalid_enum",
            f"Invalid {name}: {value}. Allowed: {sorted(allowed)}",
        )


def commit_rating(score: dict[str, int]) -> str:
    philosophy_fit = score["philosophy_fit"]
    merge_risk = score["merge_risk"]
    behavior_regression_risk = score["behavior_regression_risk"]
    maintenance_cost = score["maintenance_cost"]
    upstream_alignment = score["upstream_alignment"]
    confidence = score["confidence"]

    if philosophy_fit <= 1 or merge_risk >= 4 or behavior_regression_risk >= 4:
        return "block"
    if confidence <= 1 or maintenance_cost >= 3 or merge_risk >= 3:
        return "needs-review"
    if upstream_alignment <= 1 or behavior_regression_risk >= 2:
        return "watch"
    return "safe"


def score_breakdown(score: dict[str, int]) -> dict[str, Any]:
    rating = commit_rating(score)
    risk_flags: list[str] = []
    strengths: list[str] = []

    if score["philosophy_fit"] <= 1:
        risk_flags.append("low-philosophy-fit")
    elif score["philosophy_fit"] >= 3:
        strengths.append("strong-philosophy-fit")

    if score["merge_risk"] >= 4:
        risk_flags.append("merge-risk-critical")
    elif score["merge_risk"] >= 3:
        risk_flags.append("merge-risk-elevated")
    else:
        strengths.append("merge-risk-contained")

    if score["behavior_regression_risk"] >= 4:
        risk_flags.append("behavior-risk-critical")
    elif score["behavior_regression_risk"] >= 2:
        risk_flags.append("behavior-risk-watch")
    else:
        strengths.append("behavior-risk-contained")

    if score["maintenance_cost"] >= 3:
        risk_flags.append("maintenance-cost-elevated")
    else:
        strengths.append("maintenance-cost-contained")

    if score["upstream_alignment"] <= 1:
        risk_flags.append("upstream-alignment-low")
    elif score["upstream_alignment"] >= 3:
        strengths.append("upstream-alignment-strong")

    if score["confidence"] <= 1:
        risk_flags.append("confidence-low")
    elif score["confidence"] >= 3:
        strengths.append("confidence-strong")

    return {
        "rating": rating,
        "strengths": strengths,
        "risk_flags": risk_flags,
        "summary": (
            "eligible-for-controlled-automation"
            if rating == "safe" and not risk_flags
            else "manual-review-preferred"
        ),
    }


def needs_challenger_review(rating: str, score: dict[str, int]) -> bool:
    return rating in {"needs-review", "block"} or score["confidence"] <= 1


def phase_index(phase: str) -> int:
    require_enum("phase", phase, RUN_PHASES)
    return RUN_PHASE_SEQUENCE.index(phase)
