---
name: syncing-upstream-fork
description: Orchestrates upstream sync runs for this fork using .sync JSON5 state and `uv run python -m syncflow ...`. Use when checking upstream/main drift, starting a sync run, scoring upstream commits, forcing challenger review, or finalizing a recorded run.
---

# Skill: syncing-upstream-fork

Use `REFERENCE.md` first. Keep prompts to subagents in English. Keep user-facing summaries in Simplified Chinese.

## What this skill does

- Orchestrates one upstream sync run at a time.
- Uses `.sync/` as only structured state source.
- Uses `uv run python -m syncflow ...` as only structured state read/write path.
- Forces challenger review for human decisions.

## Do not use for

- General product feature work
- Unrelated refactors
- Direct manual edits inside `.sync/`

## Procedure

1. Read current `.sync` state through CLI.
2. If no active run exists, run `preflight`, then prefer `start-run` so upstream lock and sync branch creation stay inside one runtime path.
3. Enumerate candidate commits with `list-candidates`, or use `intake-candidates` to create commit records from that range in one step.
4. Ask `fork-policy-auditor` and `commit-score-reviewer` for evidence-based analysis.
5. Route only controlled low-risk items to `docs-policy-agent`, `workflow-merge-agent`, or `rust-merge-agent` when `auto-resolve-candidates` marks them eligible.
5. For risky or human-decided items, require `decision-challenger-agent` review.
6. Use `report-drafter-agent` to turn high-risk items into editable report drafts.
7. Use `update-report`, `list-reports`, and `archive-report` to revise and separate report views.
8. Use `decision-apply-agent` after challenger review completes and a structured decision exists.
9. Use `consensus-review-agent` to record disagreement level before application.
10. Advance phases explicitly when orchestration needs a clean checkpoint.
11. Finalize only when no blocking items remain, all decisions are resolved, and challenger review is completed via `run-finalizer-agent`.

## Rules

- Never directly edit `.sync/*.json5`.
- Prefer summaries plus relative paths in user replies.
- Treat commit messages as hints, not truth.
- Trust diff, paths, policy, and challenger review over single-agent confidence.

## Final response

- Summarize stage, blockers, next step.
- Include relative paths for reports or summaries.

## Command templates

```sh
uv run python -m syncflow get-active-run
uv run python -m syncflow preflight --base-branch main --upstream-remote upstream --upstream-branch main
uv run python -m syncflow start-run --base-branch main --upstream-remote upstream --upstream-branch main --run-id <run-id>
uv run python -m syncflow init-run --base-branch main --sync-branch sync/upstream-YYYY-MM-DD --upstream-head <sha> --merge-base <sha>
uv run python -m syncflow abort-run --run-id <run-id> --reason "<reason>" --delete-local-branch --keep-remote-branch
uv run python -m syncflow list-candidates --run-id <run-id>
uv run python -m syncflow intake-candidates --run-id <run-id>
uv run python -m syncflow advance-phase --run-id <run-id> --target-phase classify
uv run python -m syncflow score-commit --run-id <run-id> --commit-id CMP-0001 --agent commit-score-reviewer --philosophy-fit 2 --merge-risk 1 --behavior-regression-risk 2 --maintenance-cost 1 --upstream-alignment 4 --confidence 2
uv run python -m syncflow auto-resolve-candidates --run-id <run-id>
uv run python -m syncflow auto-resolve-commit --run-id <run-id> --commit-id CMP-0001 --agent docs-policy-agent --summary "<summary>"
uv run python -m syncflow create-report --run-id <run-id> --commit-id CMP-0001 --title "<report title>"
uv run python -m syncflow update-report --run-id <run-id> --report-id RPT-0001 --body "<report body>"
uv run python -m syncflow list-reports --run-id <run-id> --status active
uv run python -m syncflow get-report-by-id --run-id <run-id> --report-id RPT-0001
uv run python -m syncflow record-decision --run-id <run-id> --target-type report --target-id RPT-0001 --chosen-action keep-zzz --rationale-summary "<summary>" --human-feedback "<feedback>"
uv run python -m syncflow record-debate --run-id <run-id> --target-id DEC-0001 --trigger-reason human-decision --recommendation escalate-to-human --disagreement-level medium
uv run python -m syncflow complete-debate --run-id <run-id> --debate-id DEB-0001 --status completed --disagreement-level mostly-resolved
uv run python -m syncflow consensus-review --run-id <run-id> --decision-id DEC-0001 --debate-id DEB-0001 --disagreement-level mostly-resolved --summary "<summary>" --no-escalate-to-human
uv run python -m syncflow apply-decision --run-id <run-id> --decision-id DEC-0001 --applied-by decision-apply-agent --applied-summary "<summary>" --applied-path <path>
uv run python -m syncflow archive-report --run-id <run-id> --report-id RPT-0001
uv run python -m syncflow cleanup-sync-branch --run-id <run-id> --delete-local --keep-remote
uv run python -m syncflow finalize-run --run-id <run-id> --outcome completed --merge-to-base --delete-local-branch --keep-remote-branch
```
