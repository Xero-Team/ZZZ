# Adversarial Review Reference

Normative material for `SKILL.md`. Persona cards, output schemas, synthesis
rules, and the report template live here so the main skill stays loadable.

## Contents

Read `Scope` and `Persona cards` before dispatching anything. Then read only the
sections the current round needs.

| If you are | Read |
|---|---|
| writing the dispatch prompt | `Dispatch prompt shape` and `Round 1 output schema` |
| collecting round-2 output | `Round 2 output schema` |
| merging findings by cross-confirmation | `Synthesis rules`, `Confidence classes`, `Severity mapping` |
| writing the report | `Report template` |
| about to publish a finding | `Anti-patterns` |

`Sources` is provenance only; read it when a rule's origin is questioned.

## Scope

This reference is for reviewing a diff in this repository, with the repository's
own gates as the authority. It does not describe a service, a CI product, or a
remote review API. Nothing here needs the network.

## Persona cards

Each card is pasted into the dispatch prompt verbatim. Do not paraphrase them
into one general reviewer; the separation is the mechanism.

### Auditor

You verify that the change does what it claims, and that it is correct on the
main path.

- Trace each changed behavior end to end: input, state, error path, output.
- Check the claim in the commit message and the pull request body against the
  code, and report the gap when they disagree.
- Look for missing cases: an empty collection, a failed spawn, a cancelled task,
  a partial write, an error that is logged but not propagated.
- Do not propose architecture changes, and do not comment on naming or style.

### Adversary

You look for what breaks when the input is hostile, the ordering is unlucky, or
the invariant is violated.

- Attack `unsafe` boundaries first: aliasing, validity, initialization, layout,
  lifetime extension, and any invariant stated only in a comment.
- Look for a safe API that can reach undefined behavior, a widened `unsafe`
  scope, or an invariant quieted with `#[allow]` or `debug_assert!`.
- Look for a path that reintroduces a rejected surface: telemetry, accounts,
  billing, hosted AI, auto-update, or a default network call.
- Look for a concurrency hazard: a re-entered entity update, a dropped task that
  was supposed to run, a timer that races the executor.
- Do not restate ordinary error-handling style, and do not invent an exploit for
  code with no `unsafe` and no trust boundary. Say the section does not apply.

### Pragmatist

You judge whether the change is the smallest thing that solves the stated
problem, and whether it will still be maintainable afterwards.

- Identify over-engineering: an abstraction with one caller, a trait with one
  implementation, a setting nobody asked for, a fallback that hides a bug.
- Identify the opposite failure: duplication that will drift, a copy of a rule
  the repository already enforces in one place.
- Check the blast radius against the tier: a `high` tier change needs the
  evidence the policy asks for.
- Do not demand a refactor the user did not ask for. Report it as a lead.

## Round 1 output schema

Every persona returns exactly one JSON object, no prose around it.

```json
{
  "persona": "auditor|adversary|pragmatist",
  "verdict": "approve|conditional|reject",
  "summary": "one sentence",
  "findings": [
    {
      "severity": "P0|P1|P2",
      "file": "crates/x/src/y.rs",
      "line": 123,
      "title": "short noun phrase",
      "detail": "two to six sentences",
      "fix": "smallest safe fix, or null"
    }
  ]
}
```

`file` and `line` are mandatory. A finding that cannot name a location is
dropped rather than downgraded: `REFERENCE.md` in `hunting-code-smells` already
requires `file:line` for the same reason.

## Round 2 output schema

```json
{
  "persona": "auditor|adversary|pragmatist",
  "validate": [{ "from": "auditor", "title": "…", "reason": "…" }],
  "challenge": [{ "from": "auditor", "title": "…", "reason": "…" }],
  "added": []
}
```

`added` uses the round-1 finding shape. A `challenge` needs a concrete reason
(the code does not do that, the case is unreachable, a gate already covers it),
not a restatement of the opposite opinion.

## Dispatch prompt shape

One prompt per persona, dispatched concurrently in a single message.

```text
You are the <persona> reviewer for the diff below. Do not edit files, do not
dispatch subagents, and do not review anything outside the diff.

Repository rules that outrank your judgement:
<paste the relevant lines from .rules and the philosophy boundary>

Scope: <one sentence>
Tier: <low|medium|high> (from script/check-review-tier)

Return only the round-1 JSON object from the schema. No prose.

<source block>
```

The source block is identical in every prompt, produced once. Include the file
list with line counts so a persona can notice a file it never saw.

## Synthesis rules

Write the tally down. It is the audit trail for the verdict.

For each finding, deduplicate on `file:line` plus normalized title (lowercase,
strip punctuation, collapse whitespace), then record:

- `reported_by`: personas that raised it in round 1
- `validated_by`: personas that validated it in round 2 and did not report it
- `challenged_by`: personas that challenged it in round 2

Classify in this order, first match wins:

| Class          | Condition                                        |
| -------------- | ------------------------------------------------ |
| cross-validated | `validated_by` is non-empty and `challenged_by` is empty |
| consensus      | every persona reported it                         |
| disputed       | `challenged_by` is non-empty                      |
| solo           | reported once, nobody validated or challenged     |

`consensus` ranks below `cross-validated` on purpose: the same persona raising
the same point twice is not a second reader. Agreement from a persona that did
not report the finding is the signal.

Precedence for the report and for fixing: cross-validated, consensus, solo,
disputed. Disputed findings are last but never deleted; the ruling explains why
the code stands.

Verdict:

- any persona returns `reject` and the finding survives synthesis: `BLOCK`
- findings survive, none blocking: `SHIP-WITH-CAVEATS`
- no cross-validated, consensus, or solo findings: `SHIP`

Never let a persona's overall `verdict` field override the synthesis. A persona
can reject the change for a finding that two others challenge.

## Confidence classes

- **cross-validated**: act on it. Fix, or record why not.
- **consensus**: act on it, but read the detail first; identical reasoning from
  two personas can share one blind spot.
- **solo**: report it with the reporter named. The user decides.
- **disputed**: report both sides and your ruling. Do not silently drop it.

## Severity mapping

Use the tiers already defined in `hunting-code-smells/REFERENCE.md:34-46`.
Nothing else.

| Persona severity | Meaning in this repository                                       |
| ---------------- | ---------------------------------------------------------------- |
| P0               | a hard rule, soundness, or the philosophy boundary is violated   |
| P1               | an idiom or structural regression with a contained fix           |
| P2               | polish, safe to defer                                            |

## Report template

Write in Simplified Chinese, keep identifiers in English.

```markdown
## 对抗式审查报告

**范围**: <diff / branch / 文件列表>
**风险等级**: <low|medium|high>（依据 `script/check-review-tier`）
**审查轮次**: 2（上限 2）
**结论**: BLOCK / SHIP-WITH-CAVEATS / SHIP
**审查者**: auditor、adversary（如有降级，在此说明）

### 交叉确认（cross-validated）

1. `crates/x/src/y.rs:123` — <标题>
   - 支持: <validate 的 persona 与理由>
   - 问题: <一句话>
   - 修复: <最小安全修复>

### 一致（consensus）

1. ...

### 单方（solo）

1. `file:line` — <标题>（提出者: <persona>）

### 争议（disputed）

1. `file:line` — <标题>
   - 支持: <...>
   - 反对: <...>
   - 裁决: <为什么代码可以保持现状，或需要修改>

### 未发现问题

<零发现时必须有此节，并明确写出 "未发现问题"；不要留空>

### 已修复

- `file:line` — <改了什么>

### 剩余最高风险

- <一个>
```

## Anti-patterns

- Running a fourth model pass to break a tie between personas.
- Reporting a finding without `file:line`.
- Counting the same persona's round-1 and round-2 opinion as two votes.
- Letting a persona dispatch its own reviewer.
- Mixing models across personas and calling the result independent.
- Presenting a single surviving persona as a panel.
- Running a third analysis round instead of adjudicating.
- Editing files during review, or reviewing outside the requested diff.
- Reporting a style nit as P0 because two personas both dislike it.

## Sources

Design inputs, checked 2026-11-17:

- `addyosmani/adverse` (MIT) — three personas, a cross-examination round, and a
  deterministic synthesizer that counts validate/challenge edges instead of
  running a judge model:
  <https://github.com/addyosmani/adverse/blob/main/skills/adverse-review/SKILL.md>
- `obra/superpowers` (MIT) — scoped re-review, round caps, and the rule that a
  reviewer never dispatches another reviewer:
  <https://github.com/obra/superpowers/blob/main/skills/subagent-driven-development/task-reviewer-prompt.md>
- `obra/superpowers` (MIT) — the fix loop with per-task round limits and
  controller adjudication:
  <https://github.com/obra/superpowers/blob/main/skills/subagent-driven-development/references/fix-loop.md>
- Anthropic's account of its own review process: multiple agents hunt in
  parallel, a separate vote filters false positives, and reviews must converge.
  Where this repository departs: no hosted reviewer, no judge model, no service.
