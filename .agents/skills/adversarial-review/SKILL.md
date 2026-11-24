---
name: adversarial-review
description:
  "Runs a multi-persona adversarial review of a diff, then groups the findings
  by cross-confirmation instead of asking a judge model. Use when the user asks
  for an adversarial, multi-perspective, or panel review, or before landing a
  high-tier change to gates, CI, dependency pins, locales, or project rules. Do
  not use for trivial diffs or for absorbing upstream."
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

Read `Scope` and `Persona cards` in `REFERENCE.md` before dispatching anything,
then the section the current round needs. The same file carries the two output
schemas, the synthesis rules, and the report template. A finding without a
`file:line` and a severity is noise.

# Adversarial Review

You review one diff from several independent angles, then let the agreement
between those angles decide what is real. Two reviewers that agree are evidence;
one reviewer that is confident is not.

## Purpose

Catch the bugs a single pass misses, and suppress the findings a single pass
invents, without spending a third model call on a judge. The counts are the
verdict.

## Communication Rules

- Keep this skill prompt written in English.
- Interact with the user in Simplified Chinese.
- Write the review report in complete English for identifiers and in Simplified
  Chinese for prose. Do not compress the report.
- When a technical term could be ambiguous, add a short Chinese gloss followed
  by the English term in parentheses.

## When to Use

- The user asks for an adversarial, multi-perspective, panel, or second-opinion
  review.
- A change touches a `high` tier path under
  `docs/src/development/review-policy.md` and is about to land.
- A change to `unsafe`, FFI, or manual allocation needs more than one reader.
- One reviewer has an obvious blind spot: it wrote the code, or it reviewed the
  same area for the previous round.

Do not use this skill for typos, formatting, dependency bumps where the lockfile
diff is mechanical, or upstream absorption (use `absorbing-upstream`). Say so
and review normally instead.

## Operating Rules

- Project hard rules outrank persona output. `.rules`, `clippy.toml`, the
  locale gates, and `cargo xtask check philosophy` decide P0 findings; a persona
  cannot vote a rule violation away.
- Never run a judge model, a tie-breaker model, or a "fourth opinion" pass. If
  the counts are ambiguous, report the finding as `disputed` and let the user
  decide.
- Every reviewer is read-only. Personas do not edit files, and they do not
  dispatch subagents of their own; the orchestrator owns dispatch. A persona
  that spawns a reviewer duplicates a seat at full cost and its verdict does not
  count.
- Persona instructions go in the dispatch prompt. The `subagent` tool has no
  system-prompt slot and no model override, so do not plan around either.
- Same persona set, same model for all reviewers. Mixing models across personas
  collapses the orthogonality the design depends on.
- Keep the tones apart. If both personas read like the same general reviewer,
  the run is wasted; say so and re-dispatch rather than synthesizing.
- Findings map onto the existing severity model: P0, P1, P2 from
  `hunting-code-smells/REFERENCE.md:34-46`. Do not invent a third scale.
- Absence over configuration: a finding that recommends re-adding telemetry,
  accounts, billing, hosted AI, auto-update, or a default network call is not a
  finding. Drop it and say why.
- Scope discipline. Review the diff the user named. Do not expand into adjacent
  modules, and do not fix what the user did not ask to fix.

## Prerequisites

- A built toolchain if the run will include mechanical checks, so
  `cargo xtask clippy` and `cargo fmt --all -- --check` can run.
- A target: a diff, a branch, or a path the user named. `git status --porcelain`
  and `git diff --stat` are the cheap ways to confirm.
- Subagent capacity for two or three parallel reviewers.

## Procedure

1. Scope. Pin the target and state it in one sentence so the user can redirect.
   Compute the tier with `script/check-review-tier --base main`, or from the
   path list when no branch is involved.
2. Pick the persona set. Two personas by default; add the third when the diff is
   high tier, touches `unsafe`, or the user asked for the full panel. See
   `REFERENCE.md` for what each persona covers.
3. Collect once. Produce the source block and the file list, and embed the same
   source block in every persona prompt so they judge the same bytes. Never let
   a persona fetch the diff for itself.
4. Round 1. Dispatch every persona in one message so they run concurrently. Each
   returns the round-1 JSON from `REFERENCE.md` and nothing else.
5. Validate the round-1 output. A malformed reply gets one retry with the parser
   error appended. A second failure drops that persona. With fewer than two
   survivors, stop and report that the run is degraded rather than presenting a
   single opinion as a panel.
6. Round 2. Dispatch every surviving persona again with all round-1 findings
   included. Each validates what it agrees with, challenges what it thinks is
   wrong, and adds only what it can anchor to `file:line`. Skip this round only
   when the user asks for a single-round review.
7. Synthesize by hand using the precedence table in `REFERENCE.md`. Do not call
   a model to merge the results. Deduplicate on `file:line` plus normalized
   title, then classify each finding as cross-validated, consensus, solo, or
   disputed.
8. Adjudicate. Two reviewer rounds is the cap. After the cross-review, do not
   run a third analysis round; rule on anything still disputed, record the
   ruling, and move on.
9. Apply fixes only if the user asked for them. Fix cross-validated findings
   first. A fix that needs a redesign is reported and proposed, not done.
10. Verify. Re-run the checks that cover what changed, and report the command and
    its actual result. A re-review after fixes counts as the final round and
    covers only the fix diff.
11. Report in Simplified Chinese using the template in `REFERENCE.md`, with the
    verdict line, counts by confidence, and the top findings in precedence order.

## Examples

```text
Run an adversarial review of the changes on this branch and tell me what blocks.
```

```text
Review crates/zzz/src/ for a safety regression with the full panel.
```

## Troubleshooting

- A persona repeats another persona's wording: the tone separation failed.
  Re-dispatch with the persona card restated, do not synthesize the clone.
- Both personas report the same finding from the same file with the same
  reasoning: that is consensus, not cross-validation. Say which it is; the
  distinction is the whole point.
- A persona reports findings outside the requested scope: drop them from the
  verdict and list them once as out-of-scope observations.
- Subagent dispatch fails or is unavailable: fall back to a single-pass review
  with `hunting-code-smells` and label the report as single-reviewer. Do not
  fabricate a second opinion.

## Limitations

- The review is static. A claim that needs a running app is reported as
  unverified, not fixed.
- Two personas are not two models. Shared training makes correlated blind spots
  likely, so cross-validation is a noise filter, not proof of correctness.
- The synthesis is manual arithmetic. A miscounted run is a wrong run; the
  per-finding counts in the report exist so it can be audited.
- This skill does not run CI, approve changes, or count human approvals.

## Final Response

Summarize in Simplified Chinese:

- scope and tier reviewed, and whether the run was degraded
- the verdict line and the counts by confidence class
- findings grouped by severity, each as `file:line` + smell + fix
- what was fixed, what is reported only, and what is disputed with your ruling
- commands run and their PASS/FAIL
- the single highest-value remaining risk
