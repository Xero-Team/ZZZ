---
title: Code Review Policy
description: "How ZZZ scales review by risk: tiers, required evidence, review rounds, and the repository settings that enforce them."
---

# Code Review Policy

`CONTRIBUTING.md` requires an AI review on every change and a human review on
anything authored here. This page says how much review a change gets, what the
author must show, and when a review stops.

The policy scales by risk, not by author. A documentation fix does not need the
same attention as a change to the philosophy gate, and treating them the same
spends reviewer time where it changes nothing.

## Risk tiers

`script/check-review-tier` classifies a change by the paths it touches. The
highest matching path wins; a path that matches nothing falls back to `medium`.

| Tier     | Paths                                                                                                                                                                                             | Human approvals | Owners review |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------- | ------------- |
| `low`    | `docs/**`, `legal/**`, `*.md`                                                                                                                                                                     | 1               | no            |
| `medium` | `crates/**`, `extensions/**`, `assets/**`, and anything unmatched                                                                                                                                 | 1               | no            |
| `high`   | `.github/**`, `script/**`, `tooling/**`, `.agents/**`, dependency pins (`Cargo.toml`, `Cargo.lock`, `vendor/**`, toolchain and lint configs), `.rules`, `assets/locales/**`, the decision ledgers | 2               | yes           |

Run it locally before opening a pull request:

```sh
script/check-review-tier --base main
```

The map lives in the script, not in a configuration file. A change that lowers
a tier has to edit `script/check-review-tier`, and `script/**` is high tier, so
that change needs the review it is trying to avoid. That is the whole
anti-bypass mechanism, and it only works while the map stays in code.

## Required evidence

The pull request template asks for four fields, and `script/check-pr-metadata`
fails while any of them is empty:

- **AI review**: a pasted or linked review. The `ai-reviewed` label stands in
  once a review exists.
- **Human review**: the checkbox, or the `human-reviewed` / `upstream` label.
- **Risk**: what breaks if the change is wrong, and who notices first.
- **How to revert**: the command or steps that undo it, or `N/A`.

The tests field is expected but not enforced; a green CI job is the evidence
that matters there.

## Review rounds

A review converges or it does not. A reviewer that keeps reporting findings it
already reported burns the author's cycles without lowering risk, and an agent
author and an agent reviewer can ping-pong indefinitely if nothing stops them.

- A change gets at most three review rounds. One round is a full pass plus the
  fixes that answer it.
- After the first round, re-review only the new diff.
- After the third round, the reviewer stops reporting new findings and says
  which part of the approach needs re-evaluation instead.
- A finding is never dropped silently. Either the change fixes it, the report
  records it as open, or it becomes an issue.

`.agents/skills/hunting-code-smells/SKILL.md` and
`.agents/skills/adversarial-review/SKILL.md` both implement this cap.

## Adversarial review

For high-tier changes, `.agents/skills/adversarial-review/` runs two reviewer
personas in parallel, cross-examines their findings, and groups the result by
whether anyone else confirmed it. It does not run a judge model: the counts are
the verdict.

## What still needs a repository setting

Two parts of this policy cannot be expressed as a file in the repository:

1. **Approval counts.** Branch protection decides how many approvals a pull
   request needs. Per-tier counts need a required status check, and the tier
   script reports rather than blocks, so today the tier is advisory.
2. **Merge queue.** GitHub's merge queue is enabled in branch protection rules.
   It is the only layer that proves the checks passed on `main` with the change
   applied, which is what stops two green pull requests from breaking `main`.

`.github/CODEOWNERS` names the owner for the same paths the `high` tier covers,
so a change that can weaken a gate cannot land without that owner's approval.
It deliberately has no catch-all `*` entry: that would put one person in front
of every pull request, which is the bottleneck this policy exists to avoid.

Keep the two lists in sync. `script/check-review-tier` decides the tier;
`.github/CODEOWNERS` decides who must approve it. A path in one and not the
other means the tier claim and the enforcement disagree.

A CODEOWNERS line whose owner has no write access is ignored by GitHub without
failing anything, so the entry has to name a real account or team.
