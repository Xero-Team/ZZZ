---
name: auditing-upstream
description:
  "Re-audits ZZZ's recorded Zed upstream absorption decisions. Use when
  reviewing past A/B/C classifications, re-checking rejected (C) commits,
  correcting stale upstream-sync-*.md reports, backfilling or verifying
  docs/src/development/upstream-rejected.tsv, or writing
  upstream-sync-audit-YYYY-MM-DD.md."
license: CC-BY-4.0
compatibility: opencode
metadata:
  prompt-language: English
  user-language: Simplified Chinese
---

Follow `REFERENCE.md` in this skill first. If a re-checked commit is
philosophy-safe but needs absent architecture, keep it `C`; never import
rejected machinery so a port compiles.

# Auditing Upstream

You re-audit ZZZ's recorded upstream decisions. Forward absorption belongs
to the `absorbing-upstream` skill; this skill corrects the record and
recovers commits that were rejected too quickly.

## Communication Rules

- Keep this skill prompt written in English.
- Interact with the user in Simplified Chinese.
- Write reports, commit messages, and git trailers in English.
- When a technical term could be ambiguous, add a short Chinese gloss
  followed by the English term in parentheses.

## When to Use

- The user asks to re-audit upstream sync records or re-check `C`
  decisions.
- The user wants the rejection ledger verified, backfilled, or repaired.
- The user wants a new or continued
  `docs/src/development/upstream-sync-audit-*.md`.

Do not use this skill for forward batches. Use `absorbing-upstream` for
those.

## Operating Rules

- Conservative reviewer, not a merger. A re-check that cannot compile on
  current ZZZ stays `C`.
- Every reclassification names a local commit or a concrete reason. No
  silent edits.
- Correct the report table, narrative, and counts. Add a dated correction
  note to the report scope instead of rewriting history.
- Regenerate the ledger with `./script/backfill-upstream-ledger`. Never
  hand-edit `upstream-rejected.tsv`.
- Empty stale `script/upstream-ledger-allowlist` entries once the report
  is corrected.
- Do not add an `upstream` remote, push, open a PR, or run
  `script/cherry-pick`.

## Procedure

1. Read `README.md`, `AGENTS.md`, `.agents/skills/absorbing-upstream/`
   `SKILL.md` and `REFERENCE.md`, every
   `docs/src/development/upstream-sync-*.md`, the latest audit report, and
   this skill's `REFERENCE.md`.
2. Preflight: clean worktree, then run `./script/check-upstream-ledger`.
3. Cross-reference every recorded `C` against later local `sync:` commits
   and the current ZZZ call chain, then re-run the isolation test in
   `REFERENCE.md`.
4. Port genuinely missed commits as `B` with `git commit -s`. Revert a
   failed attempt completely and keep it `C`.
5. Correct the report tables, narratives, and counts, and update the
   baseline in the `absorbing-upstream` `REFERENCE.md`.
6. Run `./script/backfill-upstream-ledger` and
   `./script/check-upstream-ledger`. Remove stale allowlist entries.
7. Write `docs/src/development/upstream-sync-audit-YYYY-MM-DD.md` and
   format only that file with Prettier.

## Final Response

Summarize in Simplified Chinese:

- audited range and number of decision rows
- reclassifications with their local commits
- report and ledger corrections
- checks run and their PASS/FAIL
- any hard stop and why
