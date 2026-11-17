# OpenCode Skill Creation Reference

Use this reference when creating skills for this repository. Prefer the OpenCode
skills rules first, then the local repository conventions.

## Contents

- Scope for this skill
- Skill files
- Minimum markdown shape
- Recognized frontmatter fields
- Naming rules
- Description rules
- Trigger testing
- Conciseness rules
- Recommended body structure
- Progressive disclosure
- When to add `REFERENCE.md`
- One-level reference rule
- When to add supporting folders
- Iteration guidance
- Repository conventions to preserve
- Common mistakes
- Example directory layout
- Sources

## Scope For This Skill

- Create all generated skills under `.agents/skills/`.
- Treat this as a project-level workflow, not a global user profile workflow.
- Keep this reference and generated skill prompts in English.
- Keep user interaction in Simplified Chinese.

## Skill Files

OpenCode can discover skills from several locations, including:

- project agent-compatible: `.agents/skills/<name>/SKILL.md`
- global agent-compatible: `~/.agents/skills/<name>/SKILL.md`
- project OpenCode config: `.opencode/skills/<name>/SKILL.md`
- global OpenCode config: `~/.config/opencode/skills/<name>/SKILL.md`
- Claude-compatible paths under `.claude/skills/` and `~/.claude/skills/`

This workflow always uses `.agents/skills/<name>/SKILL.md`.

## Minimum Markdown Shape

```md
---
name: git-release
description: "Creates consistent releases and changelogs. Use when drafting release notes or proposing a version bump."
license: CC-BY-4.0
compatibility: opencode
version: 1.0.0
metadata:
  author: Xero Team <maintainers@example.com>
  tags:
    - release
  prompt-language: English
  user-language: Simplified Chinese
---

## Purpose

Turn merged work into a tagged release with a consistent changelog.

## What I do

- Draft release notes
- Propose a version bump
```

## Recognized Frontmatter Fields

V2 interprets these fields:

- `description`: the discovery surface; a skill without one is not advertised
- `name`: the display label only; the skill ID comes from the path
- `metadata.opencode/autoinvoke`: `false` hides the skill from the model's
  available list while keeping it loadable by ID
- `disable-model-invocation`: `true` has the same effect as the above

V2 accepts the portability fields `license` and `compatibility` but does not
interpret them. Repository convention keeps `license: CC-BY-4.0` and
`compatibility: opencode` on every skill, plus a `metadata` map carrying
`prompt-language` and `user-language`.

Every skill also carries the `agentskills.io` identity fields that the
SkillEvaluator Tier 1 gate reads: `version` in `MAJOR.MINOR.PATCH` form,
`metadata.author` as `Name <email>`, and `metadata.tags` with one to five
lowercase tags. A missing `metadata.author` fails the schema gate; a missing
`version` or `tags` costs quality points. Use a placeholder mailbox such as
`maintainers@example.com`, never a personal or upstream contact address.

All frontmatter is optional at runtime. Add a clear `description` whenever the
model should discover the skill, and do not add fields OpenCode does not read
unless this repository's convention requires them.

## Naming Rules

The skill ID comes from the path, not from `name`: `git-release/SKILL.md` and
`git-release.md` both become `git-release`, and a nested `teams/release/SKILL.md`
becomes `release`. Keep the ID and directory name aligned:

- 1 to 64 characters
- lowercase alphanumeric segments separated by single hyphens
- no leading or trailing `-`
- no consecutive `--`

OpenCode recommends this shape but does not currently enforce it. As a
repository rule, the directory name and the `name` field also match exactly.

Gerund naming is recommended when it fits naturally, for example
`creating-skills` or `processing-pdfs`. This is a style preference, not a hard
requirement. Do not rename an existing skill unless the user asks for it or the
current name is materially weak.

Equivalent regex:

```text
^[a-z0-9]+(-[a-z0-9]+)*$
```

## Description Rules

The description is the main trigger surface. It should:

- say what the skill does
- say when it should be used
- include nearby task shapes or phrases that should trigger it
- stay specific enough to beat vague generic skills
- use third person wording rather than first person or direct address
- front-load concrete keywords, filenames, or paths when they matter for
  discovery

OpenCode itself does not enforce a maximum length, but this repository documents
one: `docs/src/ai/skills.md` records a 1,024-character description limit, a 500
line body, and a 50 KB budget shared by every skill's name and description. Treat
those as the working budget. When a description outgrows it, move the trigger
detail into the body and keep only the phrases the model must match.

Good pattern:

```yaml
description: Creates or refines project-local OpenCode-compatible skills under .agents/skills/. Use when creating a new SKILL.md, deciding whether to add REFERENCE.md, or improving trigger quality for an existing local skill.
```

Weak pattern:

```yaml
description: Helps with skills.
```

## Trigger Testing

The description is judged against the requests a user actually types, not against
an idea of the task. Test it before trimming the body:

1. Collect five to eight requests with real detail: repository paths, crate
   names, gate names, file names, error text. "Tighten the skill description"
   tests less than "this skill never triggers when I ask it to review the
   locale catalogs".
2. Split them into should-trigger and near-miss should-not-trigger. The near
   misses are the valuable half: a request that shares vocabulary with the skill
   but needs a different workflow, such as asking `creating-skills` for
   application code or asking `hunting-code-smells` to absorb upstream.
3. Exclude one-step requests from the positive set. The model handles a simple
   rename or lookup without loading a skill, so such a request measures nothing.
4. For each miss, change the description: add the user's own phrasing, name the
   path or command the request contains, or add a sentence that excludes the
   near miss.
5. Record the results. There is no local trigger-evaluation harness, so the
   written requests and their outcomes are the evidence behind the description.

Keep the sets small and stable. Ten well-chosen requests beat fifty paraphrase
variants, and a description that only passes self-invented examples is still
unproven.

## Conciseness Rules

- Assume the model already knows common background.
- Keep only task-shaping information that improves outputs in this repository.
- Prefer one recommended path over a menu of alternatives.
- Use stricter instructions for fragile items such as paths, frontmatter, and
  naming.
- Move durable rules, templates, and long examples out of `SKILL.md` before the
  main file becomes bloated.

## Recommended Body Structure

This repository tends to write skills with a clear sectioned structure. A good
default is:

1. Title or role statement
2. `## Purpose`
3. `## Communication Rules` when language or platform constraints matter
4. `## When to Use`
5. `## Operating Rules`
6. `## Prerequisites`
7. `## Procedure`
8. `## Examples`
9. `## Troubleshooting`
10. `## Limitations`
11. `## Final Response`

Sections 6, 8, 9, and 10 keep the SkillEvaluator quality score honest and, more
importantly, give the reader the setup, failure, and boundary information that
the procedure alone omits. Keep each to a few bullets; drop a section rather
than padding it. Script-based skills also need `## Available scripts` with a
`| Script | Purpose | Arguments |` table, because that is where the arguments
are documented.

Keep the prompt body in English and state Simplified Chinese interaction rules
explicitly when they matter.

## Progressive Disclosure

Keep `SKILL.md` focused on role, trigger conditions, procedure, and outputs.
Move durable reference material into `REFERENCE.md` so the main skill stays easy
to load and adapt.

## When To Add REFERENCE.md

Add `REFERENCE.md` only when it reduces confusion or keeps the main skill lean,
for example when the skill needs:

- naming conventions or path rules
- long templates or schema descriptions
- command, API, or file format references
- multiple project-specific policies that should not clutter the main workflow

If the extra file does not materially improve reuse, keep everything in
`SKILL.md`.

## One-Level Reference Rule

Link reference files directly from `SKILL.md`. Avoid chains such as
`SKILL.md -> guide.md -> details.md`, because nested references are easier to
miss and weaken progressive disclosure.

## When To Add Supporting Folders

Additional folders such as `scripts/`, `references/`, or `assets/` are optional.
Add them only when they remove repeated work or carry durable resources the
skill will actually use.

Do not add speculative scaffolding.

## Iteration Guidance

- Tighten `description` before adding more body text when trigger quality is the
  real problem.
- Prefer small structural edits over full rewrites unless the current design is
  fundamentally wrong.
- When improving an existing skill, preserve name and path unless the user asks
  for a rename.
- Provide one or two realistic test prompts when they help validate discovery.

## Repository Conventions To Preserve

- Keep prompts concise and strongly task-shaped.
- Ask only the questions that materially block the draft.
- Prefer the smallest useful file set.
- Keep one skill focused on one reusable workflow.
- Use `REFERENCE.md` for normative material instead of inflating `SKILL.md`.

## Common Mistakes

- Mismatch between the folder name and the `name` field
- A vague description that does not trigger reliably
- Writing to a global path when the user asked for a project skill
- Packing multiple unrelated jobs into one skill
- Copying nonessential metadata fields without a compatibility reason
- Adding scripts, assets, or references that the skill never uses

## Example Directory Layout

```text
.agents/skills/
  creating-skills/
    SKILL.md
    REFERENCE.md
```

For heavier skills, a deliberate expansion can look like this:

```text
.agents/skills/
  example-skill/
    SKILL.md
    REFERENCE.md
    scripts/
    references/
```

## Sources

The trigger-testing method and the "treat the description as the product" framing
are adapted from Anthropic's `skill-creator`
(https://github.com/anthropics/skills, Apache-2.0), rewritten for OpenCode's
discovery rules rather than copied. The 500-line body, 1,024-character
description, and 50 KB catalog figures come from this repository's
`docs/src/ai/skills.md`, which describes how ZZZ loads skills.
