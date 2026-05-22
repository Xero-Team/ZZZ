---
name: creating-skills
description:
  "Creates or refines project-local OpenCode-compatible skills under
  .agents/skills/. Use when creating a new SKILL.md, deciding whether to add
  REFERENCE.md, tightening skill descriptions for discovery, renaming or
  restructuring an existing local skill, or turning a repeated workflow into a
  reusable skill for this repository."
license: CC-BY-4.0
compatibility: opencode
metadata:
  prompt-language: English
  user-language: Simplified Chinese
  output-target: .agents/skills
---

Follow `REFERENCE.md` in this skill first. If there is any conflict or
uncertainty, prefer the
[OpenCode skills documentation](https://opencode.ai/docs/en/skills/), then ask
the user a short clarifying question in Simplified Chinese.

# Creating OpenCode Skills

You are creating or refining a project-local OpenCode-compatible skill for this
repository.

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
- Prefer one recommended path over many parallel options unless the task truly
  needs a branch.
- Match instruction strictness to task fragility. Use low freedom for naming,
  paths, and frontmatter; allow more freedom for wording and examples.

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
5. When improving an existing skill, prefer small structural edits over a full
   rewrite unless the current design is fundamentally wrong.
6. Write imperative instructions. Assume the model already knows common
   background. Keep only information that improves outcomes for this
   repository.
7. If the skill needs normative reference material, put it in
   `.agents/skills/<skill-name>/REFERENCE.md` and keep links one level deep from
   `SKILL.md`.
8. Finish with a concise review of trigger quality, file placement, and whether
   the skill is still focused on one job.

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
  trigger.
- Verify the prompt body stays focused on one reusable job.
- Verify `REFERENCE.md` exists only if it adds real value.
- Verify OpenCode-specific rules still hold after any Claude-inspired cleanup.

## Final Response

When the draft is ready, summarize in Simplified Chinese:

- what the skill does
- which files were created
- why `REFERENCE.md` was or was not added
- one or two example prompts the user can try
- the weakest remaining assumption, only if it could materially change the skill
