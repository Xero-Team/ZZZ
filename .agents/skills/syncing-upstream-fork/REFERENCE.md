# Syncing Upstream Fork Reference

## Scope

- Single responsibility: orchestrate upstream sync workflow.
- State root: `.sync/`
- Structured state writer: `uv run python -m syncflow ...`

## Phase order

1. preflight
2. init-run
3. intake
4. classify
5. score
6. auto-resolve
7. draft-reports
8. human-decision
9. challenger-review
10. consensus-review
11. apply-decisions
12. finalize

## Required CLI surface in phase one

- `init-run`
- `preflight`
- `start-run`
- `get-active-run`
- `list-candidates`
- `intake-candidates`
- `create-commit-record`
- `advance-phase`
- `score-commit`
- `create-report`
- `update-report`
- `list-reports`
- `archive-report`
- `get-report-by-id`
- `record-decision`
- `record-debate`
- `consensus-review`
- `apply-decision`
- `complete-debate`
- `mark-resolved`
- `cleanup-sync-branch`
- `finalize-run`

## Phase 2 agents

- `report-drafter-agent`
- `decision-apply-agent`
- `consensus-review-agent`
- `run-finalizer-agent`
- `workflow-merge-agent`
- `rust-merge-agent`
- `docs-policy-agent`

## Orchestration rules

- Refuse concurrent runs.
- Use `main` as base branch.
- Use `sync/upstream-YYYY-MM-DD` as sync branch naming baseline.
- User-facing output stays short and Chinese.
- Prompt files stay English.

## Command templates

### Inspect active run

```sh
uv run python -m syncflow get-active-run
```

### Initialize run

```sh
uv run python -m syncflow preflight \
  --base-branch main \
  --upstream-remote upstream \
  --upstream-branch main

uv run python -m syncflow start-run \
  --base-branch main \
  --upstream-remote upstream \
  --upstream-branch main \
  --run-id <run-id>

uv run python -m syncflow init-run \
  --base-branch main \
  --sync-branch sync/upstream-YYYY-MM-DD \
  --upstream-head <sha> \
  --merge-base <sha>

uv run python -m syncflow abort-run \
  --run-id <run-id> \
  --reason "<reason>" \
  --delete-local-branch \
  --keep-remote-branch
```

### Intake from session range

```sh
uv run python -m syncflow list-candidates --run-id <run-id>
uv run python -m syncflow intake-candidates --run-id <run-id>
uv run python -m syncflow advance-phase --run-id <run-id> --target-phase classify
```

### Intake from explicit range

```sh
uv run python -m syncflow intake-candidates \
  --run-id <run-id> \
  --from <from-sha> \
  --to <to-sha>
```

### Record human decision and challenger review

```sh
uv run python -m syncflow record-decision \
  --run-id <run-id> \
  --target-type report \
  --target-id RPT-0001 \
  --chosen-action keep-zzz \
  --rationale-summary "<summary>" \
  --human-feedback "<feedback>"

uv run python -m syncflow record-debate \
  --run-id <run-id> \
  --target-id DEC-0001 \
  --trigger-reason human-decision \
  --recommendation escalate-to-human \
  --disagreement-level medium

uv run python -m syncflow consensus-review \
  --run-id <run-id> \
  --decision-id DEC-0001 \
  --debate-id DEB-0001 \
  --disagreement-level mostly-resolved \
  --summary "<summary>" \
  --no-escalate-to-human

uv run python -m syncflow apply-decision \
  --run-id <run-id> \
  --decision-id DEC-0001 \
  --applied-by decision-apply-agent \
  --applied-summary "<summary>" \
  --applied-path <path>
```

### Finalize with merge and cleanup

```sh
uv run python -m syncflow finalize-run \
  --run-id <run-id> \
  --outcome completed \
  --merge-to-base \
  --delete-local-branch \
  --keep-remote-branch
```

## Challenger policy

- Every human decision must be recorded with pending debate state.
- Every human decision must then go through `decision-challenger-agent`.
- Agents may also request challenger review for low confidence, policy-sensitive paths, or suspicious simple diffs.

## Paths to mention in summaries

- `.sync/state.json5`
- `.sync/archive/index.json5`
- `.sync/runs/<run-id>/session.json5`
- `.sync/runs/<run-id>/index.json5`
- `.sync/runs/<run-id>/reports/`
- `.sync/runs/<run-id>/final/summary.md`
