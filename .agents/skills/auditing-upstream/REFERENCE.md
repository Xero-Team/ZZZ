# Auditing Upstream Reference

Normative rules for the `auditing-upstream` skill.

## Session Defaults

Override only when the user supplies a value.

```text
AUDIT_DATE=YYYY-MM-DD (today)
LOCAL_BASE_BRANCH=main
LEDGER=docs/src/development/upstream-rejected.tsv
ALLOWLIST=script/upstream-ledger-allowlist
AUDIT_REPORT=docs/src/development/upstream-sync-audit-YYYY-MM-DD.md
CREATE_REMOTE=no
CREATE_PR=no
PUSH=no
```

## Sources Of Truth

- The decision tables in every `upstream-sync-*.md` report.
- The rejection ledger, which is generated from those tables.
- Local git history: `Upstream:` trailers, `cherry picked from commit`
  lines, and `sync:` subjects.
- The current ZZZ call chain, for the isolation test.

When two sources disagree, git history and the current tree win. Correct
the report, then regenerate the ledger.

## Method

1. Enumerate the decision rows in every report and count `A` / `B` / `C`.
2. For every recorded `C`, search later local `sync:` commits for its SHA.
3. Re-run the isolation test for each `C` justified only by "conflict",
   "already diverged", or "not isolatable".
4. Re-check version-follow commits against the current `crates/zzz`
   version policy.
5. Record each result as a finding `F1`, `F2`, and so on.

## Isolation Test

A commit is portable only if all of these are true:

1. The retained behavior is philosophy-safe.
2. A real ZZZ caller exists, or the change is a complete local bugfix
   on an existing path.
3. It does not require an absent crate, protocol variant, or GPUI API.
4. It does not require a rejected account, collab, telemetry, or native
   agent surface.
5. After omitting rejected hunks, the safety invariant still holds and
   can be tested, or the omission is explicitly test-only.
6. It does not depend on an unabsorbed earlier commit.

If any check fails, keep the commit `C`. Do not import an unused type so a
later commit can land.

## Reclassification

| Move  | When                                                             |
| ----- | ---------------------------------------------------------------- |
| C → A | A later `git cherry-pick -x -s` landed the exact commit.          |
| C → B | A local port exists with an `Upstream:` trailer for the SHA.      |
| B → C | The port cannot compile without absent or rejected APIs.          |
| A → B | The local equivalent was only partial after later divergence.     |

Version follow: track the upstream app version as `B`. A version that a
later follow supersedes carries no separate local commit; name the
superseding commit.

## Report Shape

Create or update `AUDIT_REPORT`:

1. Scope: local base, upstream URL/ref, audited records, decision-row
   count and A/B/C totals.
2. Method: the cross-reference and isolation pass.
3. Findings: `F1`, `F2`, and so on, each with evidence.
4. Corrections applied: a table of report, correction, and local commit.
5. Ledger: rows before and after, and allowlist entries removed.

## Corrections

- Edit the report decision row to the final class and local commit.
- Update the report counts and any scope sentence that states them.
- Add a dated correction note to the report scope. Do not rewrite the
  per-commit failure narrative; append a "Superseded" line instead.
- Run `./script/backfill-upstream-ledger` and
  `./script/check-upstream-ledger`.
- Delete any `ALLOWLIST` entry the correction makes unnecessary. The
  checker fails on a stale entry.
- Format only the touched reports and the audit report with Prettier.

## Hard Stops

Stop and leave the tree clean if:

- the worktree is dirty for reasons other than owned report edits
- `./script/check-upstream-ledger` fails before the audit starts
- a port needs a rejected or absent API
- a reclassification lacks a local commit or a concrete reason

Record the finding, the reason, and the last reviewed baseline.
