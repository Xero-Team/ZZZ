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
---

## What I do

- Draft release notes
- Propose a version bump
```

## Recognized Frontmatter Fields

OpenCode recognizes these fields in `SKILL.md` frontmatter:

- `name` required
- `description` required
- `license` optional
- `compatibility` optional
- `metadata` optional string-to-string map

Unknown fields are ignored by OpenCode. For this repository, prefer the
OpenCode-native fields unless there is a deliberate cross-tool compatibility
reason to keep extra metadata.

## Naming Rules

The skill name must:

- be 1 to 64 characters long
- use lowercase alphanumeric segments separated by single hyphens
- not start or end with `-`
- not contain consecutive `--`
- match the directory name exactly

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

The OpenCode docs allow descriptions from 1 to 1024 characters. Use enough
detail to make triggering reliable without turning the description into a wall
of text.

Good pattern:

```yaml
description: Creates or refines project-local OpenCode-compatible skills under .agents/skills/. Use when creating a new SKILL.md, deciding whether to add REFERENCE.md, or improving trigger quality for an existing local skill.
```

Weak pattern:

```yaml
description: Helps with skills.
```

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
2. `## Communication Rules` when language or platform constraints matter
3. `## When to Use`
4. `## Procedure`
5. `## Review Checklist` or `## Final Response`

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
