---
name: hunting-code-smells
description:
  "Finds and fixes code smells and quality regressions in ZZZ. Use when
  reviewing a diff or module, auditing unsafe soundness, or checking
  absorbed upstream code. Do not use for upstream absorption."
license: CC-BY-4.0
compatibility: opencode
version: 1.0.0
metadata:
  author: Xero Team <maintainers@example.com>
  tags:
    - rust
    - review
    - code-quality
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
`cargo xtask check philosophy`. Your job is the part tools do not catch:
project-specific traps, idiom drift, and Clean Code structural smells.

## Purpose

Find the quality and soundness problems that `cargo xtask clippy`, the
locale gates, and the philosophy gate cannot see, then fix only the ones
in scope with the smallest safe diff.

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
- `cargo xtask check philosophy` is a hard gate. Never suggest a fix that
  reintroduces telemetry, accounts, billing, hosted AI/collab, auto-update,
  or a default network call. Delete the surface instead.
- Absence over configuration: do not add a setting, feature flag, or
  fallback that re-enables a rejected surface.
- No `mod.rs`. Prefer existing files; do not split one logical change across
  many new small files.
- No `unwrap()` or `expect()` outside tests and genuine programmer
  invariants. No `let _ =` on fallible operations. Propagate with `?`, use
  `.log_err()` / `.detach_and_log_err(cx)`, or handle explicitly.
- Soundness outranks style. An unstated `unsafe` invariant, a safe API that can
  reach undefined behavior, or an aliasing, validity, initialization, or layout
  violation is P0/P1 under `REFERENCE.md` section 2 even when the code compiles
  and reads cleanly.
- Never widen an `unsafe` scope and never quiet a soundness finding with a lint
  attribute or a `debug_assert!`. State the invariant or enforce it.
- Do not manufacture memory-safety findings for a change with no `unsafe`, raw
  pointer, FFI, or manual allocation. Say the section does not apply and move
  on.
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
- Generated and vendored trees are out of scope by default: `vendor/`,
  `target/`, and generated tree-sitter sources. A finding inside them is not
  actionable; name the generator or the dependency instead.
- Do not edit `.rules` inline. If a reusable trap emerges, put a "Suggested
  .rules additions" section in the report instead.

## Review Rounds

A review that keeps reporting old findings burns the author's cycles without
lowering risk. The report is the record, and three rounds is the cap.

- Round 1 is the full pass. Every later round reviews only the diff that
  changed since the previous round.
- After round 3, report no new findings. Say which part of the approach needs
  re-evaluation instead, and let the author decide.
- Never re-report a finding the report already lists. If it is still open, say
  it is unchanged; do not re-derive it as new.
- A finding this diff introduced and one that predates it are different
  classes. Report the second as pre-existing and do not let it block a change
  that merely sits next to it.
- Do not drop a real finding either. Fix it, list it as open, or propose it as
  a follow-up with `file:line`.

When the user wants a second opinion rather than another round, that is
`adversarial-review`, not a repeat of this skill.

## Prerequisites

- A built toolchain so `cargo xtask clippy` and `cargo fmt` run.
- `rg` for the hard-rule detectors in `REFERENCE.md` section 8.
- The target diff or module named by the user, or a clean worktree to diff.

## Procedure

1. Scope. Identify the target: `git diff --name-only` for a review, or the
   module the user named. Confirm the worktree is clean except files this
   session owns. If the scope is large, review one crate at a time.
2. Mechanical pass. Run the cheap checks in `REFERENCE.md` section 8:
   `cargo xtask clippy` (or `cargo xtask clippy -p <crate>`),
   `cargo fmt --all -- --check`, `cargo xtask check`, the locale-parity
   check, and the fast `rg` detectors for the P0 hard rules. Inventory the
   unsafe surface with the section 2 lead commands before reading the code, so
   you know whether the memory-safety contract is in scope. Record every FAIL.
 3. Manual pass. Read the target against `REFERENCE.md` in severity order:
   section 1 (P0 hard rules), section 2 (memory safety and UB contract — P0
   inside `unsafe`, FFI, and manual allocation, and P0 for a new or changed
   `unsafe` without a `// SAFETY:` invariant; P1 for a pre-existing undocumented
   invariant, which you report rather than mass-edit), section 3 (Rust idiom),
   section 4 (Clean Code structure), section 5 (GPUI/architecture), section 6
   (i18n/docs), section 7 (quality optimization leads). Cite `file:line` for
   each finding, and quote the Reference or FLS clause anchor for a
   memory-safety finding.
4. Classify. Assign P0/P1/P2 per the severity model. Drop weak or
   speculative findings instead of padding the report.
5. Fix. Change only what the user asked for, smallest safe diff. When fixing a
   smell would require a redesign, report it and propose the refactor instead
   of doing it unasked.
6. Verify. Re-run the checks from step 2 that cover the changed area, plus
   `cargo test -p <crate>` for behavior-affecting changes.
7. Report in Simplified Chinese using the template in `REFERENCE.md`.

## Examples

```text
Review the last three commits for code smells and report P0/P1 only.
```

```text
Audit the unsafe block in crates/gpui/src/platform for soundness.
```

## Troubleshooting

- `cargo xtask clippy` fails on unrelated drift: record it as pre-existing
  and keep reviewing; do not fix code the user did not ask about.
- A finding has no `file:line`: drop it; speculative findings do not belong
  in the report.
- The memory-safety pass finds nothing: confirm the change touches `unsafe`,
  FFI, or manual allocation before claiming section 2 was applied.

## Limitations

- The review is static. Behavior claims that need a runtime test are
  reported as unverified, not fixed.
- Findings are read from the current tree, so a stale checkout can hide
  drift that already landed.
- This skill does not reformat, rename, or restructure code outside the
  requested scope.

## Final Response

Summarize in Simplified Chinese:

- scope reviewed
- findings grouped by severity, each as `file:line` + smell + fix
- what was fixed vs. only reported
- checks run and their PASS/FAIL
- the single highest-value remaining risk
