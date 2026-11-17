---
name: auditing-security
description:
  "Runs a full security audit of the ZZZ codebase: reconnaissance, coverage-led
  hunting across the repo's real trust boundaries, independent validation of each
  candidate, and a validated findings record. Use when the user asks for a
  security audit, threat model, vulnerability hunt, or pen test of the whole
  codebase or a named surface. Do not use for routine diff review, upstream
  absorption, or philosophy-boundary checks."
license: CC-BY-4.0
compatibility: opencode
version: 1.0.0
metadata:
  author: Xero Team <maintainers@example.com>
  tags:
    - security
    - audit
    - orchestration
  prompt-language: English
  user-language: Simplified Chinese
---

Follow `REFERENCE.md` in this skill first. It carries the trust-boundary model,
the ZZZ attack-surface catalog, the severity mapping onto P0/P1/P2, and the
`findings.json` schema. A candidate without a named principal, a crossed boundary,
and an observable result is not a finding.

# Auditing Security

You run a full security audit against this repository's actual trust boundaries,
then prove each finding with source evidence and an independent reader. This is
defensive and source-first: the target source is read-only, execution is limited
to what the repository already builds, and a claim you cannot prove is recorded as
unverified rather than promoted.

## Purpose

Answer "is there a security failure here, and where exactly" with a coverage
ledger, a validated findings record, and a report that states what was not covered.
One run never exhausts the target.

## Communication Rules

- Keep this skill prompt written in English.
- Interact with the user in Simplified Chinese.
- Write findings, JSON, and report prose in complete English for identifiers and
  Simplified Chinese for narrative. Do not compress a finding.
- When a technical term could be ambiguous, add a short Chinese gloss followed by
  the English term in parentheses.

## When to Use

- The user asks for a security audit, vulnerability hunt, threat model, or pen test.
- A named surface needs a full review: extensions, agent tools, local IPC, the
  agent panel, remote development, the Telegram panel, or a settings parser.
- A security regression is suspected across several crates.

Do not use this skill for a diff review (use `hunting-code-smells`), for a
multi-persona second opinion (use `adversarial-review`), for absorbing upstream
(use `absorbing-upstream`), or for enforcing the absence boundary (use
`enforcing-philosophy-absence`). Say which skill fits and stop.

## Operating Rules

- The target source is read-only. This audit never edits the repository, and it
  never commits. Fixes are described in the finding, not applied.
- Write only inside the run workspace created by `scripts/security_audit.py init`.
  It refuses a path inside the repository; do not work around that refusal.
- No network access, no dependency installs, no probing a deployed endpoint, and no
  calling a provider API. ZZZ is local-first; treat any network call as out of scope
  evidence and say so.
- Execute only what the repository already builds: `cargo test -p <crate>`,
  `cargo run` where the user asked for it, and existing fixtures. Never execute
  code from `vendor/`, a downloaded extension, or an untrusted project's skills.
- Philosophy boundary outranks a checklist. A finding that recommends re-adding
  telemetry, accounts, billing, hosted AI, auto-update, or a default network call
  is not a finding: drop it and say why. `cargo xtask check philosophy` encodes the
  boundary.
- Severity is the existing model: P0/P1/P2 from
  `hunting-code-smells/REFERENCE.md`. Do not invent a second scale, and do not
  assign severity to `needs_validation`.
- A `confirmed` finding needs the six boundary fields, a `path:line` evidence
  entry, the invariant plus the smallest fix, and a verifier whose id differs from
  the finder's.
- The agent that validates a candidate is never the agent that found it. Dispatch a
  fresh subagent and embed the candidate in the prompt; do not let the verifier go
  hunting.
- Every ledger unit ends in a state: `covered`, `candidate`, `deferred`, `blocked`,
  or `out_of_scope`. A unit the budget cannot reach is `deferred` with a reason,
  never silently dropped.
- Cap agent invocations before starting. If the budget cannot fund the remaining
  verification, stop hunting and record `run_status: "incomplete"` with the exact
  reason.
- Two terminal states only: all five phases complete with both validators passing,
  or `incomplete` with its reason disclosed in the report.
- Do not present a scoped or single run as a clean bill of health. State the
  coverage, the gaps, and the actions that were never executed.

## Prerequisites

- `python3` for the run workspace and validators.
- `git` for the pinned source ref, and the pinned Rust toolchain for any test the
  audit runs.
- Subagent capacity for hunters and for independent verification.
- A decision from the user on profile (`quick` or `standard`) and scope when they
  did not name one.

## Procedure

1. **Setup.** Resolve the repository root, `git rev-parse HEAD`, and whether the
   worktree is dirty. Choose the run workspace outside the repository and create it:
   `python3 scripts/security_audit.py init --workspace <path> --repo <root> --run-id run-1`.
   Record `source_ref` and the profile in `run-metadata.json`.
2. **Reconnaissance.** Read `REFERENCE.md` attack surfaces against the tree, then
   seed `coverage-ledger.json` with units shaped
   `<surface>.<boundary>.<attack_class>`. Cover the surfaces in scope and mark the
   rest `out_of_scope` with a reason. Validate:
   `python3 scripts/security_audit.py check-ledger --ledger <path>`.
3. **Hunting waves.** Assign units to subagents in priority order (lowest-trust
   entry first, then the boundaries guarding code execution, credentials, and
   agent authority). Each hunter receives its units, the relevant `REFERENCE.md`
   attack-class block, and the structured-result contract; it returns candidates
   and uncovered surfaces only. Run one coverage-critic pass per wave to add
   missing units, then update the ledger.
4. **Candidate validation.** Deduplicate candidates by root cause, then dispatch a
   fresh verifier per candidate to try to disprove it. The verifier confirms the
   source path, checks whether the control exists elsewhere, and fills
   `verification` and `verified_by`. Unresolved facts become `needs_validation`
   with the exact blocker.
5. **Verification and output.** Write `findings.json`, then validate:
   `python3 scripts/security_audit.py check-findings --findings <path> --ledger <path> --repo <root>`.
   Re-run a fresh verifier on any record that changed after validation. Print the
   counts with `summary`.
6. **Reporting.** Write `REPORT.md` in the workspace from the final records: scope
   and source ref, coverage statement, confirmed findings by severity, unverified
   hypotheses with their blockers, rejected candidates, and the limits of the run.
   Quote the validator output.

## Available scripts

All are standard-library Python and operate on files only; run each with `--help`
first.

| Script | Purpose | Arguments |
| --- | --- | --- |
| `scripts/security_audit.py init` | Create the run workspace and the three shared files; refuses a path inside the repository or a non-empty directory. | `--workspace --repo --run-id` |
| `scripts/security_audit.py check-ledger` | Validate unit ids, states, owners, and reasons. | `--ledger` |
| `scripts/security_audit.py check-findings` | Validate verdicts, boundary fields, evidence paths, fix shape, and finder/verifier separation. | `--findings [--ledger] [--repo] [--allow-missing]` |
| `scripts/security_audit.py summary` | Print status, severity, and unit-state counts for the report. | `--findings [--ledger]` |
| `scripts/test_security_audit.py` | Tests for the validator rules. | `python3 -m unittest test_security_audit` |

## Examples

```text
Run a security audit of the extension host and the agent tool surface.
```

```text
Threat-model how a `zzz://skill` link reaches the agent, then check the loader.
```

## Troubleshooting

- `check-findings` rejects a confirmed finding: the usual cause is a missing
  `boundary.result`, evidence outside the repository, or the same id in `found_by`
  and `verified_by`.
- A verifier agrees with the finder without new evidence: it repeated the claim.
  Re-dispatch with the instruction to disprove, and drop the candidate if it stays
  unproven.
- The ledger cannot be finished in budget: mark the untouched units `deferred` with
  `budget_cannot_reserve_validation`, then report the run as incomplete.
- A surface needs a runtime only the user has: record a `needs_validation` with the
  exact missing observation and a safe manual check, instead of guessing.
- The audit drifts into upstream absorption or a feature request: stop, name the
  correct skill, and hand the finding over as a follow-up.

## Limitations

- Source review cannot prove runtime behavior. Anything that needs a running GUI,
  an OS sandbox profile, or a packaged bundle is `needs_validation`.
- Coverage is bounded by the ledger: surfaces not seeded are not audited, and the
  report must say so.
- Correlated blind spots remain. Independent agents share a model and a codebase, so
  agreement is a noise filter, not proof.
- This skill does not fix, commit, open a pull request, or run CI.

## Final Response

Summarize in Simplified Chinese:

- scope, source ref, and profile, and whether the run was complete or incomplete
- coverage: units by state, and what stayed `out_of_scope`
- confirmed findings by P0/P1/P2, each as `file:line` plus the one-line invariant it
  breaks
- unverified hypotheses with their blockers, and rejected candidates with the reason
- validator output as PASS/FAIL, and the paths of the run artifacts
- the single highest remaining risk
