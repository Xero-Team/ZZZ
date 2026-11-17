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

`description` is the only field a discoverable subagent needs; every other
frontmatter field is optional at runtime. For this repository always set
`description` and `mode`.

- `description`: the discovery surface the model reads when it chooses an
  agent; include concrete trigger phrases
- `mode`: `primary`, `subagent`, or `all`. A new custom agent defaults to
  `primary` when the field is omitted
- `model`: `provider/model`, with an optional `#variant`
- `permissions`: ordered `{ action, resource, effect }` rules
- `steps`: maximum model steps before OpenCode removes tools and asks for a
  summary
- `hidden`: removes the agent from listings and the subagent catalog; this is
  visibility, not security
- `color`: six-digit hex display color
- `disabled`: removes a built-in or custom agent at that point in configuration

Do not write the deprecated V1 top-level fields `tools`, `permission`,
`temperature`, `top_p`, `prompt`, `disable`, or `maxSteps`. Put the prompt in
the Markdown body, which becomes the agent's `system` prompt.

## Mode Selection

- `primary`: an alternate main working mode
- `subagent`: a specialized helper used by another agent or invoked with `@`
- `all`: available as both primary and subagent

Default to `subagent` unless the user clearly wants a main working mode.

## Tool Model

Built-in tools are `read`, `glob`, `grep`, `edit`, `write`, `patch`, `shell`,
`webfetch`, `websearch`, `question`, `skill`, `subagent`, and `execute`.
Availability is not a frontmatter switch in V2; behavior is controlled by
`permissions`.

Authoring-critical rules:

- `edit` permission governs file modification tools including `edit`, `write`,
  and `patch`
- `shell` permission governs command execution
- `patch` is offered only to supported models; other models receive `edit` and
  `write`

Do not describe `write` as independently safe if `edit` is denied.

## Built-In Tools To Consider

- `read`: read file contents or a non-recursive directory listing
- `glob`: find files by pattern
- `grep`: search file contents by regular expression or literal text
- `edit`: modify existing files
- `write`: create new files or overwrite existing ones; controlled by `edit`
- `patch`: apply a multi-file patch; controlled by `edit`
- `shell`: run shell commands in the host user's shell
- `question`: ask the user concise clarification questions
- `webfetch`: fetch a known URL
- `websearch`: discover information on the web
- `skill`: load a skill file
- `subagent`: launch a configured child session
- `execute`: run JavaScript in Code Mode to compose catalog tools

## Tool Choice Heuristics

Use the narrowest set that fits the job.

- Review / analysis agents: usually `read`, `grep`, `glob`; maybe `webfetch`
- Planning agents: usually `read`, `grep`, `glob`, `question`
- Documentation agents: may add `edit`; add `shell` only if command execution is
  explicitly part of the job
- Debug agents: may add `shell`; only add `edit` when fixes are in scope
- Implementation agents: may add `edit` and `shell` if the task truly requires
  both

## Permission Model

`permissions` is an ordered list of rules. Each rule has an `action`, a
`resource`, and an `effect` of `allow`, `ask`, or `deny`. The last matching rule
wins, so put broad rules before exceptions.

```yaml
permissions:
  - action: "*"
    resource: "*"
    effect: allow
  - action: edit
    resource: "*"
    effect: deny
  - action: shell
    resource: "*"
    effect: deny
```

Actions that matter for authoring: `shell` (command execution), `edit` (`edit`,
`write`, and `patch`), `subagent` (child agents), `read`, `glob`, `grep`,
`webfetch`, `websearch`, and `skill`. Global rules apply before agent rules, so
an agent can refine a broader global policy.

For shell commands, narrow the `resource` to the command text instead of
granting broad `shell` access:

```yaml
permissions:
  - action: shell
    resource: "*"
    effect: ask
  - action: shell
    resource: "git status *"
    effect: allow
  - action: shell
    resource: "grep *"
    effect: allow
```

Prefer command-specific `shell` rules when the user wants shell access with
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
- Granting `shell` or `edit` without a task-driven reason
- Forgetting that `write` and `patch` are covered by `edit`
- Writing a V1 `tools` or `permission` map instead of V2 `permissions` rules
- Using `websearch` when the task already has a concrete URL

## Example: Read-Only Reviewer

```md
---
description: "Reviews code for bugs, regressions, and maintainability risks"
mode: subagent
permissions:
  - action: edit
    resource: "*"
    effect: deny
  - action: shell
    resource: "*"
    effect: deny
---

You are a read-only review agent.
```

## Example: Constrained Docs Agent

```md
---
description: "Writes and maintains project documentation files"
mode: subagent
permissions:
  - action: edit
    resource: "*"
    effect: allow
  - action: shell
    resource: "*"
    effect: deny
---

You are a documentation agent.
```
