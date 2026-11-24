---
name: creating-skills
description:
  "Creates or refines project-local OpenCode skills under .agents/skills/.
  Use when authoring a SKILL.md, adding a REFERENCE.md, or tightening an
  existing skill. Do not use for application code."
license: CC-BY-4.0
compatibility: opencode
version: 1.0.0
metadata:
  author: Xero Team <maintainers@example.com>
  tags:
    - skills
    - opencode
  prompt-language: English
  user-language: Simplified Chinese
  output-target: .agents/skills
---

Read the `REFERENCE.md` sections the task needs; the Contents block maps a task
to its sections. If there is any conflict or uncertainty, prefer the
[OpenCode skills documentation](https://opencode.ai/docs/en/skills/), then ask
the user a short clarifying question in Simplified Chinese.

# Creating OpenCode Skills

You are creating or refining a project-local OpenCode-compatible skill for this
repository.

## Purpose

Package one reusable workflow as a small, discoverable skill whose
description triggers on the right requests and whose body stays cheap to
load.

## Communication Rules

- Keep this skill prompt written in English.
- Interact with the user in Simplified Chinese.
- Write all user-facing summaries, questions, examples, and follow-up
  suggestions in Simplified Chinese.
- When a technical term could be ambiguous, you may add a short Chinese gloss
  followed by the English term in parentheses.
- Keep the generated skill prompt in English unless the user explicitly asks for
  another language.

## When to Use

- Use this skill when the user wants a new reusable skill, a rename, a trigger
  fix, a tighter `description`, or a structural cleanup of an existing local
  skill.
- Use this skill when the work belongs under `.agents/skills/` rather than
  `.opencode/skills/` or a global profile.
- Do not use this skill for general application code, generic documentation, or
  OpenCode config changes outside skill authoring.

## Operating Rules

- Always create project-level skills under `.agents/skills/<skill-name>/`.
- Never write this workflow to `.opencode/skills/`, user-global locations, or
  any other folder unless the user explicitly asks to change the target.
- The skill directory name and the `name` field in `SKILL.md` must match
  exactly.
- Prefer the smallest useful file set.
- Start with `SKILL.md`, then add `REFERENCE.md` only when it keeps the main
  skill lean.
- Keep each skill focused on one reusable workflow, not a vague category.
- Preserve OpenCode-compatible frontmatter and path rules even when borrowing
  authoring ideas from Claude skill guidance.
- Keep `SKILL.md` concise. Move durable rules, long examples, and naming or path
  policies into `REFERENCE.md`.
- Keep `description` within the documented 1,024-character limit, and keep the
  body under 500 lines. The catalog of every skill name and description shares a
  50 KB budget (`docs/src/ai/skills.md`), so one long description hides other
  skills from the model.
- Prefer one recommended path over many parallel options unless the task truly
  needs a branch.
- Match instruction strictness to task fragility. Use low freedom for naming,
  paths, and frontmatter; allow more freedom for wording and examples.

## Prerequisites

- The target directory `.agents/skills/` exists or can be created.
- `REFERENCE.md` is available for naming, frontmatter, and layout rules.
- No OpenCode configuration change is needed to publish the skill.

## Procedure

1. Inspect the conversation and extract only the signals that matter:
   - the workflow or capability to package
   - when the skill should trigger
   - who will use it
   - expected outputs or artifacts
   - required tools or resources
   - tools or behaviors to avoid
   - whether the task is greenfield or improving an existing skill
2. If key information is missing, ask only the shortest blocking question in
   Simplified Chinese. Prioritize exact job, trigger contexts, expected output,
   need for reference material, and tool or path constraints.
3. Decide the smallest useful file set:
   - `SKILL.md` only for short self-contained workflows
   - `SKILL.md` plus `REFERENCE.md` when rules, templates, examples, or project
     conventions would otherwise bloat the main file
   - extra folders only when they remove repeated work now, not speculatively
4. Draft or refine `SKILL.md` first. Tighten `description` before adding body
   text if discovery is weak.
5. Test the trigger before polishing the body. Write five to eight realistic
   requests, mix should-trigger with near-miss should-not-trigger, and check
   each one against the description alone. See "Testing the Trigger" below.
6. When improving an existing skill, prefer small structural edits over a full
   rewrite unless the current design is fundamentally wrong.
7. Write imperative instructions. Assume the model already knows common
   background. Keep only information that improves outcomes for this
   repository.
8. If the skill needs normative reference material, put it in
   `.agents/skills/<skill-name>/REFERENCE.md` and keep links one level deep from
   `SKILL.md`.
9. Finish with a concise review of trigger quality, file placement, and whether
   the skill is still focused on one job.

## Testing the Trigger

The model sees only the `name` and `description` until it loads the skill, and it
tends to undertrigger rather than overtrigger. Treat the description as the
product and the body as the manual, then check it against real phrasing:

1. Write five to eight requests the user would actually type, with real paths,
   crate names, and gate names, not abstract labels like "create a chart".
2. At least half must be near misses: requests that share keywords with the
   skill but need a different workflow, so the test measures precision, not
   just recall.
3. Mark each request `should-trigger` or `should-not-trigger`, then read the
   description against every one and record the miss.
4. Fix a miss in the description, not in the body. Add the phrase the user
   actually used, or add the scope sentence that excludes the near miss.
5. Do not keep a simple one-step request as a positive test: the model handles
   those without a skill and will not load one regardless of the description.

A description that survives this pass is the strongest evidence available in
this repository. There is no local trigger-evaluation harness, so the recorded
prompts are the evidence, and they belong in the final response.

## Output Expectations

- Create or update `.agents/skills/<skill-name>/SKILL.md`.
- Add `REFERENCE.md` only when it materially improves reuse or keeps
  `SKILL.md` concise.
- When useful, include one or two suggested prompts that can test whether the
  skill triggers and behaves as intended.

## Review Checklist

- Verify the folder path is `.agents/skills/<skill-name>/`.
- Verify `SKILL.md` is uppercase and the `name` matches the folder.
- Verify the `description` is concrete, third-person, and specific enough to
  trigger, and that it fit the documented 1,024-character limit.
- Verify the description passed the near-miss trigger test, and record the
  requests that failed, if any.
- Verify the prompt body stays focused on one reusable job.
- Verify `REFERENCE.md` exists only if it adds real value.
- Verify OpenCode-specific rules still hold after any Claude-inspired cleanup.

## Examples

```text
Create a skill for our release checklist.
```

```text
This skill never triggers; tighten its description.
```

## Troubleshooting

- The skill never triggers: front-load concrete task words and paths in
  `description`, and keep the `name` equal to the directory name.
- The frontmatter is invalid: `description` and `name` must parse as YAML
  scalars, so quote a value that contains a colon.
- The skill loads but ignores its references: link them directly from
  `SKILL.md`; nested reference chains get skipped.
- The body keeps growing: move durable rules and templates into
  `REFERENCE.md` instead of appending sections.

## Limitations

- Trigger quality is checked against written prompts, not measured by a harness;
  only a real session proves the description works.
- OpenCode ignores fields it does not read, so extra frontmatter is
  documentation, not behavior.
- This skill writes project-local skills only and does not publish them.

## Final Response

When the draft is ready, summarize in Simplified Chinese:

- what the skill does
- which files were created
- why `REFERENCE.md` was or was not added
- the trigger test: which requests should trigger, which near misses should not,
  and any that the description still misses
- one or two example prompts the user can try
- the weakest remaining assumption, only if it could materially change the skill
