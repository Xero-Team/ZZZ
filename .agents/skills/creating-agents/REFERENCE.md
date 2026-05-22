# OpenCode Agent Authoring Reference

Use this reference when creating or refining agents for this repository. Prefer
the agents rules first, then the tools rules.

## Scope For This Skill

- Create all generated agents under `.opencode/agents/`.
- Treat this as a project-level workflow, not a global user profile workflow.
- Keep skill prompts and generated agent prompts in English.
- Keep user interaction in Simplified Chinese.

## Agent Files

Markdown-defined OpenCode agents can be placed in:

- global: `~/.config/opencode/agents/`
- project-level: `.opencode/agents/`

This skill always uses `.opencode/agents/`.

The Markdown filename becomes the agent name. For example, `review.md` creates
an agent named `review`.

## Minimum Markdown Shape

```md
---
description: "Reviews code for quality and best practices"
mode: subagent
---

You are a code review agent.
```

## Required And Common Fields

Required:

- `description`: required discovery surface; include concrete trigger phrases

Common:

- `mode`: `primary`, `subagent`, or `all`
- `tools`: enable or disable tools for the agent
- `permission`: control whether tool use is allowed, denied, or asks first
- `model`: override model selection when needed
- `temperature`: lower for deterministic analysis, higher for exploratory work
- `steps`: cap agent iterations when cost or speed matters
- `hidden`: only for `subagent`; hides it from `@` autocomplete

## Mode Selection

- `primary`: an alternate main working mode
- `subagent`: a specialized helper used by another agent or invoked with `@`
- `all`: available as both primary and subagent

Default to `subagent` unless the user clearly wants a main working mode.

## Tool Model

OpenCode tools are enabled by configuration, but behavior is controlled by
permissions.

Authoring-critical rule:

- `edit` permission governs file modification tools including `edit`, `write`,
  and `patch`

Do not describe `write` as independently safe if `edit` is denied.

## Built-In Tools To Consider

- `read`: read file contents
- `grep`: search file contents by regex
- `glob`: find files by pattern
- `edit`: modify existing files; also governs `write` and `patch`
- `write`: create new files or overwrite existing ones; controlled by `edit`
- `patch`: apply diffs; controlled by `edit`
- `bash`: run shell commands
- `question`: ask the user concise clarification questions
- `webfetch`: fetch a known URL
- `websearch`: discover information on the web
- `skill`: load a skill file
- `todowrite`: manage a todo list; disabled for subagents by default unless
  explicitly enabled
- `lsp`: experimental code intelligence if available in the environment

## Tool Choice Heuristics

Use the narrowest set that fits the job.

- Review / analysis agents: usually `read`, `grep`, `glob`; maybe `webfetch`
- Planning agents: usually `read`, `grep`, `glob`, `question`
- Documentation agents: may add `edit`; add `bash` only if command execution is
  explicitly part of the job
- Debug agents: may add `bash`; only add `edit` when fixes are in scope
- Implementation agents: may add `edit` and `bash` if the task truly requires
  both

## Permission Model

Permissions support these values:

- `allow`
- `ask`
- `deny`

You can define them globally or per agent. Relevant examples:

```yaml
permission:
  edit: deny
  bash: ask
  webfetch: allow
```

For shell commands, command-specific rules can be narrower than broad `bash`
access:

```yaml
permission:
  bash:
    "*": ask
    "git status *": allow
    "grep *": allow
```

Prefer command-specific bash permissions when the user wants shell access with
guardrails.

## Web Tool Split

- Use `websearch` for discovery when the target source is not known yet.
- Use `webfetch` when the URL is already known and the agent needs retrieval,
  not discovery.

## Search Tool Caveat

`grep` and `glob` rely on ripgrep behavior. Ignore rules such as `.gitignore`
can hide files unless the environment explicitly allows them.

## Good Authoring Defaults

- Make `description` concrete and searchable.
- Start from the most restrictive useful tool set.
- Use `subagent` for narrowly scoped helpers.
- Keep prompt bodies focused on one job.
- Add `hidden: true` only for internal helpers that should not clutter user
  autocomplete.

## Common Mistakes

- Writing a vague `description` that is hard to discover
- Choosing `all` when only one mode is needed
- Granting `bash` or `edit` without a task-driven reason
- Forgetting that `write` and `patch` are covered by `edit`
- Adding `todowrite` assumptions to subagents without enabling it explicitly
- Using `websearch` when the task already has a concrete URL

## Example: Read-Only Reviewer

```md
---
description: "Reviews code for bugs, regressions, and maintainability risks"
mode: subagent
permission:
  edit: deny
  bash: deny
---

You are a read-only review agent.
```

## Example: Constrained Docs Agent

```md
---
description: "Writes and maintains project documentation files"
mode: subagent
permission:
  edit: allow
  bash: deny
---

You are a documentation agent.
```
