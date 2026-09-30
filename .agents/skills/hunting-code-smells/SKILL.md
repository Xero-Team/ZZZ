---
name: hunting-code-smells
description:
  "Finds and fixes code smells and quality regressions in ZZZ. Use when
  reviewing a diff or module for code quality, hunting code smells,
  refactoring, auditing an absorbed upstream change for cleanliness, or when
  asked whether Rust code is idiomatic, clean, or over-engineered. Covers ZZZ
  hard rules (.rules, clippy.toml, i18n, GPUI, philosophy gate), Clean Code
  heuristics, and Rust idiom rules."
license: CC-BY-4.0
compatibility: opencode
metadata:
  prompt-language: English
  user-language: Simplified Chinese
---

Follow `REFERENCE.md` in this skill first. Findings are only useful when they
name a concrete `file:line`, a severity, and the smallest safe fix. Prefer a
short accurate report over a wide speculative refactor.

# Hunting Code Smells

You review ZZZ code for smells and quality regressions, then fix only what the
user asked for. ZZZ already enforces a strict baseline through
`script/clippy`, `[workspace.lints]`, `clippy.toml`, `.rules`, and
`script/check-philosophy`. Your job is the part tools do not catch:
project-specific traps, idiom drift, and Clean Code structural smells.

## Communication Rules

- Keep this skill prompt written in English.
- Interact with the user in Simplified Chinese.
- Write code comments, commit messages, and the review report in complete
  English. Do not compress the report.
- When a technical term could be ambiguous, add a short Chinese gloss
  followed by the English term in parentheses.

## When to Use

- Reviewing a diff, branch, PR, or named module for code quality.
- Hunting code smells, dead code, duplication, or over-abstraction.
- Auditing an absorbed upstream commit after cherry-pick.
- The user asks whether code is idiomatic, clean, lean, or over-engineered.

Do not use this skill to absorb upstream (use `absorbing-upstream`), to run
the philosophy removal pass (use `enforcing-philosophy-absence`), or to author
unrelated documentation.

## Operating Rules

- Project hard rules come first. A P0 rule from `.rules`, `clippy.toml`,
  `AGENTS.md`, or the philosophy gate outranks any idiom preference.
- `script/check-philosophy` is a hard gate. Never suggest a fix that
  reintroduces telemetry, accounts, billing, hosted AI/collab, auto-update,
  or a default network call. Delete the surface instead.
- Absence over configuration: do not add a setting, feature flag, or
  fallback that re-enables a rejected surface.
- No `mod.rs`. Prefer existing files; do not split one logical change across
  many new small files.
- No `unwrap()` or `expect()` outside tests and genuine programmer
  invariants. No `let _ =` on fallible operations. Propagate with `?`, use
  `.log_err()` / `.detach_and_log_err(cx)`, or handle explicitly.
- Full-word variable names. No abbreviations.
- Comments only explain a non-obvious why. Never comment what the code says.
- In `Entity::update` closures, use the inner `cx`; never re-enter the same
  entity's update.
- Dropped `cx.spawn` / `cx.background_spawn` tasks are cancelled. Await,
  detach, or store them.
- GPUI tests use GPUI executor timers, not `smol::Timer::after`.
- i18n: user-facing text goes through `i18n::tr(cx, key, fallback)`. Every
  key exists in both `assets/locales/en.json` and
  `assets/locales/zh-CN.json`, the key sets match exactly, and the in-code
  `fallback` equals the `en.json` value.
- Scope discipline. Fix the requested area only. Do not reformat untouched
  regions and do not bundle unrelated refactors.
- Do not edit `.rules` inline. If a reusable trap emerges, put a "Suggested
  .rules additions" section in the report instead.

## Procedure

1. Scope. Identify the target: `git diff --name-only` for a review, or the
   module the user named. Confirm the worktree is clean except files this
   session owns. If the scope is large, review one crate at a time.
2. Mechanical pass. Run the cheap checks in `REFERENCE.md` section 7:
   `./script/clippy` (or `./script/clippy -p <crate>`), `cargo fmt --check`,
   the `script/check-*` helpers that apply, the locale-parity check, and the
   fast `rg` detectors for the P0 hard rules. Record every FAIL.
3. Manual pass. Read the target against `REFERENCE.md` in severity order:
   section 1 (P0 hard rules), section 2 (Rust idiom), section 3 (Clean Code
   structure), section 4 (GPUI/architecture), section 5 (i18n/docs), section 6
   (quality optimization leads). Cite `file:line` for each finding.
4. Classify. Assign P0/P1/P2 per the severity model. Drop weak or
   speculative findings instead of padding the report.
5. Fix. Change only what the user asked for, smallest safe diff. When fixing a
   smell would require a redesign, report it and propose the refactor instead
   of doing it unasked.
6. Verify. Re-run the checks from step 2 that cover the changed area, plus
   `cargo test -p <crate>` for behavior-affecting changes.
7. Report in Simplified Chinese using the template in `REFERENCE.md`.

## Final Response

Summarize in Simplified Chinese:

- scope reviewed
- findings grouped by severity, each as `file:line` + smell + fix
- what was fixed vs. only reported
- checks run and their PASS/FAIL
- the single highest-value remaining risk
