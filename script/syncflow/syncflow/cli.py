from __future__ import annotations

import json
import os
from pathlib import Path
from typing import Any
from typing import Annotated

import typer

from syncflow.git import list_candidates as git_list_candidates
from syncflow.models import SyncflowError, fail, ok
from syncflow.state import SyncStateStore

app = typer.Typer(add_completion=False, no_args_is_help=True)


def repo_root() -> Path:
    override = os.getenv("SYNCFLOW_REPO_ROOT")
    if override:
        return Path(override).resolve()
    return Path(__file__).resolve().parents[3]


def store() -> SyncStateStore:
    return SyncStateStore(repo_root())


def print_json(payload: dict[str, object]) -> None:
    typer.echo(json.dumps(payload, indent=2, ensure_ascii=False))


def candidate_range_from_session(run_id: str) -> tuple[str, str]:
    session = store().require_active_run(run_id)
    candidate_range = session["candidate_commit_range"]
    if ".." not in candidate_range:
        raise SyncflowError(
            "invalid_candidate_range", f"Invalid candidate range: {candidate_range}"
        )
    from_ref, to_ref = candidate_range.split("..", 1)
    return from_ref, to_ref


def run_command(fn: Any) -> None:
    try:
        result = fn()
    except SyncflowError as error:
        print_json(fail(error.error_code, error.message))
        raise typer.Exit(code=1) from error
    except Exception as error:  # noqa: BLE001
        print_json(fail("unexpected_error", str(error)))
        raise typer.Exit(code=1) from error
    print_json(ok(result))


@app.command("init-run")
def init_run(
    base_branch: Annotated[str, typer.Option("--base-branch")],
    sync_branch: Annotated[str, typer.Option("--sync-branch")],
    upstream_head: Annotated[str, typer.Option("--upstream-head")],
    merge_base: Annotated[str, typer.Option("--merge-base")],
    run_id: Annotated[str | None, typer.Option("--run-id")] = None,
) -> None:
    run_command(
        lambda: store().init_run(
            base_branch=base_branch,
            sync_branch=sync_branch,
            upstream_head=upstream_head,
            merge_base=merge_base,
            run_id=run_id,
        )
    )


@app.command("preflight")
def preflight(
    base_branch: Annotated[str, typer.Option("--base-branch")] = "main",
    upstream_remote: Annotated[str, typer.Option("--upstream-remote")] = "upstream",
    upstream_branch: Annotated[str, typer.Option("--upstream-branch")] = "main",
    sync_branch: Annotated[str | None, typer.Option("--sync-branch")] = None,
) -> None:
    run_command(
        lambda: store().preflight(
            base_branch=base_branch,
            upstream_remote=upstream_remote,
            upstream_branch=upstream_branch,
            sync_branch=sync_branch,
        )
    )


@app.command("start-run")
def start_run(
    base_branch: Annotated[str, typer.Option("--base-branch")] = "main",
    upstream_remote: Annotated[str, typer.Option("--upstream-remote")] = "upstream",
    upstream_branch: Annotated[str, typer.Option("--upstream-branch")] = "main",
    sync_branch: Annotated[str | None, typer.Option("--sync-branch")] = None,
    run_id: Annotated[str | None, typer.Option("--run-id")] = None,
) -> None:
    run_command(
        lambda: store().start_run(
            base_branch=base_branch,
            upstream_remote=upstream_remote,
            upstream_branch=upstream_branch,
            sync_branch=sync_branch,
            run_id=run_id,
        )
    )


@app.command("get-active-run")
def get_active_run() -> None:
    run_command(lambda: store().get_active_run())


@app.command("abort-run")
def abort_run(
    run_id: Annotated[str, typer.Option("--run-id")],
    reason: Annotated[str, typer.Option("--reason")],
    delete_local_branch: Annotated[
        bool, typer.Option("--delete-local-branch/--keep-local-branch")
    ] = True,
    delete_remote_branch: Annotated[
        bool, typer.Option("--delete-remote-branch/--keep-remote-branch")
    ] = False,
    remote_name: Annotated[str, typer.Option("--remote-name")] = "origin",
) -> None:
    run_command(
        lambda: store().abort_run(
            run_id=run_id,
            reason=reason,
            delete_local_branch=delete_local_branch,
            delete_remote_branch_flag=delete_remote_branch,
            remote_name=remote_name,
        )
    )


@app.command("list-candidates")
def list_candidates(
    run_id: Annotated[str, typer.Option("--run-id")],
    from_ref: Annotated[str | None, typer.Option("--from")] = None,
    to_ref: Annotated[str | None, typer.Option("--to")] = None,
) -> None:
    def command() -> dict[str, object]:
        store().require_active_run(run_id)
        resolved_from = from_ref
        resolved_to = to_ref
        if resolved_from is None or resolved_to is None:
            resolved_from, resolved_to = candidate_range_from_session(run_id)
        commits = git_list_candidates(repo_root(), resolved_from, resolved_to)
        return {
            "run_id": run_id,
            "from": resolved_from,
            "to": resolved_to,
            "candidates": commits,
        }

    run_command(command)


@app.command("intake-candidates")
def intake_candidates(
    run_id: Annotated[str, typer.Option("--run-id")],
    from_ref: Annotated[str | None, typer.Option("--from")] = None,
    to_ref: Annotated[str | None, typer.Option("--to")] = None,
    replace_existing: Annotated[bool, typer.Option("--replace-existing")] = False,
    exclude_path: Annotated[list[str] | None, typer.Option("--exclude-path")] = None,
    domain: Annotated[list[str] | None, typer.Option("--domain")] = None,
) -> None:
    def command() -> dict[str, object]:
        resolved_from = from_ref
        resolved_to = to_ref
        if resolved_from is None or resolved_to is None:
            resolved_from, resolved_to = candidate_range_from_session(run_id)
        candidates = git_list_candidates(repo_root(), resolved_from, resolved_to)
        filtered_candidates = store().filter_candidates(
            candidates=candidates,
            exclude_paths=exclude_path or [],
            include_domains=domain or [],
        )
        intake_result = store().intake_candidates(
            run_id=run_id,
            candidates=filtered_candidates,
            replace_existing=replace_existing,
        )
        return {
            "run_id": run_id,
            "from": resolved_from,
            "to": resolved_to,
            "candidate_count": len(filtered_candidates),
            **intake_result,
        }

    run_command(command)


@app.command("create-commit-record")
def create_commit_record(
    run_id: Annotated[str, typer.Option("--run-id")],
    upstream_commit: Annotated[str, typer.Option("--upstream-commit")],
    title: Annotated[str, typer.Option("--title")],
    domain: Annotated[list[str] | None, typer.Option("--domain")] = None,
    risk_path: Annotated[list[str] | None, typer.Option("--risk-path")] = None,
) -> None:
    run_command(
        lambda: store().create_commit_record(
            run_id=run_id,
            upstream_commit=upstream_commit,
            title=title,
            domains=domain or [],
            risk_paths=risk_path or [],
        )
    )


@app.command("assign-domain")
def assign_domain(
    run_id: Annotated[str, typer.Option("--run-id")],
    commit_id: Annotated[str, typer.Option("--commit-id")],
    domain: Annotated[list[str] | None, typer.Option("--domain")] = None,
) -> None:
    run_command(
        lambda: store().assign_domain(
            run_id=run_id,
            commit_id=commit_id,
            domains=domain or [],
        )
    )


@app.command("write-agent-result")
def write_agent_result(
    run_id: Annotated[str, typer.Option("--run-id")],
    agent: Annotated[str, typer.Option("--agent")],
    target_type: Annotated[str, typer.Option("--target-type")],
    target_id: Annotated[str, typer.Option("--target-id")],
    status: Annotated[str, typer.Option("--status")],
    summary: Annotated[str, typer.Option("--summary")],
    payload_json: Annotated[str | None, typer.Option("--payload-json")] = None,
) -> None:
    def command() -> dict[str, object]:
        payload = json.loads(payload_json) if payload_json else {}
        if not isinstance(payload, dict):
            raise SyncflowError(
                "invalid_payload", "payload_json must decode to an object"
            )
        return store().write_agent_result(
            run_id=run_id,
            agent=agent,
            target_type=target_type,
            target_id=target_id,
            status=status,
            summary=summary,
            payload=payload,
        )

    run_command(command)


@app.command("score-commit")
def score_commit(
    run_id: Annotated[str, typer.Option("--run-id")],
    commit_id: Annotated[str, typer.Option("--commit-id")],
    agent: Annotated[str, typer.Option("--agent")],
    philosophy_fit: Annotated[int, typer.Option("--philosophy-fit")],
    merge_risk: Annotated[int, typer.Option("--merge-risk")],
    behavior_regression_risk: Annotated[
        int, typer.Option("--behavior-regression-risk")
    ],
    maintenance_cost: Annotated[int, typer.Option("--maintenance-cost")],
    upstream_alignment: Annotated[int, typer.Option("--upstream-alignment")],
    confidence: Annotated[int, typer.Option("--confidence")],
) -> None:
    run_command(
        lambda: store().score_commit(
            run_id=run_id,
            commit_id=commit_id,
            agent=agent,
            score={
                "philosophy_fit": philosophy_fit,
                "merge_risk": merge_risk,
                "behavior_regression_risk": behavior_regression_risk,
                "maintenance_cost": maintenance_cost,
                "upstream_alignment": upstream_alignment,
                "confidence": confidence,
            },
        )
    )


@app.command("create-report")
def create_report(
    run_id: Annotated[str, typer.Option("--run-id")],
    commit_id: Annotated[str, typer.Option("--commit-id")],
    title: Annotated[str, typer.Option("--title")],
) -> None:
    run_command(
        lambda: store().create_report(run_id=run_id, commit_id=commit_id, title=title)
    )


@app.command("list-commits")
def list_commits(
    run_id: Annotated[str, typer.Option("--run-id")],
    review_status: Annotated[str | None, typer.Option("--review-status")] = None,
    domain: Annotated[str | None, typer.Option("--domain")] = None,
    risk_level: Annotated[str | None, typer.Option("--risk-level")] = None,
    auto_only: Annotated[bool, typer.Option("--auto-only/--all")] = False,
) -> None:
    run_command(
        lambda: store().list_commits(
            run_id=run_id,
            review_status=review_status,
            domain=domain,
            risk_level=risk_level,
            auto_only=auto_only,
        )
    )


@app.command("auto-resolve-candidates")
def auto_resolve_candidates(
    run_id: Annotated[str, typer.Option("--run-id")],
) -> None:
    run_command(lambda: store().auto_resolve_candidates(run_id=run_id))


@app.command("auto-resolve-commit")
def auto_resolve_commit(
    run_id: Annotated[str, typer.Option("--run-id")],
    commit_id: Annotated[str, typer.Option("--commit-id")],
    agent: Annotated[str, typer.Option("--agent")],
    summary: Annotated[str, typer.Option("--summary")],
    chosen_action: Annotated[str, typer.Option("--chosen-action")] = "accept-upstream",
) -> None:
    run_command(
        lambda: store().auto_resolve_commit(
            run_id=run_id,
            commit_id=commit_id,
            agent=agent,
            summary=summary,
            chosen_action=chosen_action,
        )
    )


@app.command("update-report")
def update_report(
    run_id: Annotated[str, typer.Option("--run-id")],
    report_id: Annotated[str, typer.Option("--report-id")],
    title: Annotated[str | None, typer.Option("--title")]=None,
    body: Annotated[str | None, typer.Option("--body")]=None,
    status: Annotated[str | None, typer.Option("--status")]=None,
) -> None:
    run_command(
        lambda: store().update_report(
            run_id=run_id,
            report_id=report_id,
            title=title,
            body=body,
            status=status,
        )
    )


@app.command("list-reports")
def list_reports(
    run_id: Annotated[str, typer.Option("--run-id")],
    status: Annotated[str | None, typer.Option("--status")]=None,
) -> None:
    run_command(lambda: store().list_reports(run_id=run_id, status=status))


@app.command("update-report-index")
def update_report_index(
    run_id: Annotated[str, typer.Option("--run-id")],
    report_id: Annotated[str, typer.Option("--report-id")],
    decision_id: Annotated[list[str] | None, typer.Option("--decision-id")] = None,
    debate_id: Annotated[list[str] | None, typer.Option("--debate-id")] = None,
    status: Annotated[str | None, typer.Option("--status")] = None,
) -> None:
    run_command(
        lambda: store().update_report_index(
            run_id=run_id,
            report_id=report_id,
            decision_ids=decision_id,
            debate_ids=debate_id,
            status=status,
        )
    )


@app.command("archive-report")
def archive_report(
    run_id: Annotated[str, typer.Option("--run-id")],
    report_id: Annotated[str, typer.Option("--report-id")],
) -> None:
    run_command(lambda: store().archive_report(run_id=run_id, report_id=report_id))


@app.command("record-decision")
def record_decision(
    run_id: Annotated[str, typer.Option("--run-id")],
    target_type: Annotated[str, typer.Option("--target-type")],
    target_id: Annotated[str, typer.Option("--target-id")],
    chosen_action: Annotated[str, typer.Option("--chosen-action")],
    rationale_summary: Annotated[str, typer.Option("--rationale-summary")],
    human_feedback: Annotated[str, typer.Option("--human-feedback")],
) -> None:
    run_command(
        lambda: store().record_decision(
            run_id=run_id,
            target_type=target_type,
            target_id=target_id,
            chosen_action=chosen_action,
            rationale_summary=rationale_summary,
            human_feedback=human_feedback,
        )
    )


@app.command("record-debate")
def record_debate(
    run_id: Annotated[str, typer.Option("--run-id")],
    target_id: Annotated[str, typer.Option("--target-id")],
    trigger_reason: Annotated[str, typer.Option("--trigger-reason")],
    recommendation: Annotated[str, typer.Option("--recommendation")],
    disagreement_level: Annotated[str, typer.Option("--disagreement-level")],
    challenger_agent: Annotated[
        str, typer.Option("--challenger-agent")
    ] = "decision-challenger-agent",
) -> None:
    run_command(
        lambda: store().record_debate(
            run_id=run_id,
            target_id=target_id,
            trigger_reason=trigger_reason,
            recommendation=recommendation,
            disagreement_level=disagreement_level,
            challenger_agent=challenger_agent,
        )
    )


@app.command("advance-phase")
def advance_phase(
    run_id: Annotated[str, typer.Option("--run-id")],
    target_phase: Annotated[str, typer.Option("--target-phase")],
) -> None:
    run_command(lambda: store().advance_phase(run_id=run_id, target_phase=target_phase))


@app.command("mark-resolved")
def mark_resolved(
    run_id: Annotated[str, typer.Option("--run-id")],
    decision_id: Annotated[str, typer.Option("--decision-id")],
    resolution_status: Annotated[str, typer.Option("--resolution-status")] = "resolved",
) -> None:
    run_command(
        lambda: store().mark_resolved(
            run_id=run_id,
            decision_id=decision_id,
            resolution_status=resolution_status,
        )
    )


@app.command("apply-decision")
def apply_decision(
    run_id: Annotated[str, typer.Option("--run-id")],
    decision_id: Annotated[str, typer.Option("--decision-id")],
    applied_by: Annotated[str, typer.Option("--applied-by")],
    applied_summary: Annotated[str, typer.Option("--applied-summary")],
    applied_path: Annotated[list[str] | None, typer.Option("--applied-path")] = None,
    git_commit_sha: Annotated[str | None, typer.Option("--git-commit-sha")] = None,
) -> None:
    run_command(
        lambda: store().apply_decision(
            run_id=run_id,
            decision_id=decision_id,
            applied_by=applied_by,
            applied_summary=applied_summary,
            applied_paths=applied_path or [],
            git_commit_sha=git_commit_sha,
        )
    )


@app.command("complete-debate")
def complete_debate(
    run_id: Annotated[str, typer.Option("--run-id")],
    debate_id: Annotated[str, typer.Option("--debate-id")],
    status: Annotated[str, typer.Option("--status")] = "completed",
    disagreement_level: Annotated[str | None, typer.Option("--disagreement-level")] = None,
) -> None:
    run_command(
        lambda: store().complete_debate(
            run_id=run_id,
            debate_id=debate_id,
            status=status,
            disagreement_level=disagreement_level,
        )
    )


@app.command("consensus-review")
def consensus_review(
    run_id: Annotated[str, typer.Option("--run-id")],
    decision_id: Annotated[str, typer.Option("--decision-id")],
    debate_id: Annotated[str, typer.Option("--debate-id")],
    disagreement_level: Annotated[str, typer.Option("--disagreement-level")],
    summary: Annotated[str, typer.Option("--summary")],
    escalate_to_human: Annotated[bool, typer.Option("--escalate-to-human/--no-escalate-to-human")] = False,
) -> None:
    run_command(
        lambda: store().consensus_review(
            run_id=run_id,
            decision_id=decision_id,
            debate_id=debate_id,
            disagreement_level=disagreement_level,
            summary=summary,
            escalate_to_human=escalate_to_human,
        )
    )


@app.command("cleanup-sync-branch")
def cleanup_sync_branch(
    run_id: Annotated[str, typer.Option("--run-id")],
    delete_local: Annotated[bool, typer.Option("--delete-local/--keep-local")] = True,
    delete_remote: Annotated[bool, typer.Option("--delete-remote/--keep-remote")] = False,
    remote_name: Annotated[str, typer.Option("--remote-name")] = "origin",
) -> None:
    run_command(
        lambda: store().cleanup_sync_branch(
            run_id=run_id,
            delete_local=delete_local,
            delete_remote=delete_remote,
            remote_name=remote_name,
        )
    )


@app.command("get-run-summary")
def get_run_summary(
    run_id: Annotated[str, typer.Option("--run-id")],
) -> None:
    run_command(lambda: store().get_run_summary(run_id=run_id))


@app.command("get-report-by-id")
def get_report_by_id(
    run_id: Annotated[str, typer.Option("--run-id")],
    report_id: Annotated[str, typer.Option("--report-id")],
) -> None:
    run_command(lambda: store().get_report_by_id(run_id=run_id, report_id=report_id))


@app.command("get-history-summary")
def get_history_summary() -> None:
    run_command(lambda: store().get_history_summary())


@app.command("list-decisions")
def list_decisions(
    run_id: Annotated[str, typer.Option("--run-id")],
    resolution_status: Annotated[str | None, typer.Option("--resolution-status")] = None,
    target_type: Annotated[str | None, typer.Option("--target-type")] = None,
) -> None:
    run_command(
        lambda: store().list_decisions(
            run_id=run_id,
            resolution_status=resolution_status,
            target_type=target_type,
        )
    )


@app.command("list-debates")
def list_debates(
    run_id: Annotated[str, typer.Option("--run-id")],
    status: Annotated[str | None, typer.Option("--status")] = None,
    disagreement_level: Annotated[str | None, typer.Option("--disagreement-level")] = None,
) -> None:
    run_command(
        lambda: store().list_debates(
            run_id=run_id,
            status=status,
            disagreement_level=disagreement_level,
        )
    )


@app.command("check-run-consistency")
def check_run_consistency(
    run_id: Annotated[str, typer.Option("--run-id")],
) -> None:
    run_command(lambda: store().check_run_consistency(run_id=run_id))


@app.command("finalize-run")
def finalize_run(
    run_id: Annotated[str, typer.Option("--run-id")],
    outcome: Annotated[str, typer.Option("--outcome")] = "completed",
    merge_to_base: Annotated[bool, typer.Option("--merge-to-base/--no-merge-to-base")] = True,
    delete_local_branch: Annotated[bool, typer.Option("--delete-local-branch/--keep-local-branch")] = True,
    delete_remote_branch: Annotated[bool, typer.Option("--delete-remote-branch/--keep-remote-branch")] = False,
    remote_name: Annotated[str, typer.Option("--remote-name")] = "origin",
) -> None:
    run_command(
        lambda: store().finalize_run(
            run_id=run_id,
            outcome=outcome,
            merge_to_base=merge_to_base,
            delete_local_branch=delete_local_branch,
            delete_remote_branch_flag=delete_remote_branch,
            remote_name=remote_name,
        )
    )
