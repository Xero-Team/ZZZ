---
name: creating-agents
description:
  "Creates or refines project-local OpenCode agents under .opencode/agents/.
  Use when designing an agent or fixing its mode, tools, or permissions.
  Do not use for skills."
license: CC-BY-4.0
compatibility: opencode
version: 1.0.0
metadata:
  author: Xero Team <maintainers@example.com>
  tags:
    - agents
    - opencode
  prompt-language: English
  user-language: Simplified Chinese
  output-target: .opencode/agents
---

Follow `REFERENCE.md` in this skill first. If there is any conflict or
uncertainty, prefer the
[OpenCode agents documentation](https://opencode.ai/docs/agents/), then the
[OpenCode tools documentation](https://opencode.ai/docs/tools/), then ask the
user a short clarifying question in Simplified Chinese.

# Creating OpenCode Agents

You are creating or refining an OpenCode agent Markdown file for this
repository.

## Purpose

Produce one narrowly scoped agent whose mode, description, tools, and
permissions match its job, so the model picks it only when it helps.

## When to Use

- The user wants a new agent, or a refinement of an existing one.
- The user wants an agent's mode, tools, permissions, or description fixed.
- The user wants a specialized subagent separated from the main mode.

Do not use this skill for skills (use `creating-skills`), for general
OpenCode configuration, or for prompts that do not live under
`.opencode/agents/`.

## Operating Rules

- Keep this skill prompt written in English.
- Interact with the user in Simplified Chinese.
- Generate the final agent prompt in English.
- You may add a short Chinese gloss for a term when it prevents ambiguity, for
  example `subagent (子代理)`.
- Always create project-level agent files under `.opencode/agents/`.
- Never create agents in `.github/agents/`, user-global locations, or any other
  folder for this workflow.
- Keep this skill focused on agent authoring or refinement, not general
  OpenCode configuration changes.
- When choosing mode, tools, or permissions, consult `REFERENCE.md` before
  guessing.
- Prefer the narrowest capability set that still fits the requested job.

## Prerequisites

- The target directory `.opencode/agents/` exists or can be created.
- `REFERENCE.md` is available for mode, tool, and permission semantics.
- No OpenCode configuration change is needed outside that directory.

## Extract From Conversation

First, inspect the conversation and gather the strongest available signals.
Extract:

- the specialized role or persona
- the concrete job scope
- whether this is a new agent or a refinement of an existing one
- when this agent should be picked over the default agent
- likely mode: `primary`, `subagent`, or `all`
- preferred tools
- tools that should be avoided
- desired permission posture: read-only, constrained write, or broad execution

If the conversation already makes these clear, do not ask unnecessary questions.

## Clarify Only What Matters

If the request is underspecified, ask concise follow-up questions in Simplified
Chinese. Prioritize the following gaps:

1. What exact job should this agent do?
2. Should it be a main working mode or a specialized helper?
3. Which capabilities are required: reading, searching, editing, writing,
   patching, shell commands, web fetch, web search, asking questions, loading
   skills, or launching subagents?
4. Which capabilities must be avoided?
5. Should the agent be conservative by default?

Do not ask all questions at once if only one missing answer blocks the draft.

## Choose The Mode

Use these defaults unless the user says otherwise:

- Choose `subagent` for a specialized helper, a role likely to be invoked with
  `@name`, or a task that should stay isolated from the main conversation.
- Choose `primary` for an alternate main working mode the user may switch into.
- Choose `all` only when the user explicitly wants the same agent to be
  available both as a main mode and as a callable helper.

## Choose Tools And Permissions

Consult `REFERENCE.md` for exact semantics before drafting tool or permission
fields.

Default to minimal capabilities:

- Reviewer / analyzer: prefer read-only behavior, usually `read`, `grep`,
  `glob`, and sometimes `webfetch`. Usually deny `edit` and `shell`.
- Planner: prefer read-only behavior with optional `question`. Usually deny
  `edit` and `shell`.
- Docs writer: may allow `edit` when the job includes writing docs. Do not add
  `shell` unless the user explicitly wants command execution.
- Debugger: may need `shell`, `read`, `grep`, and `glob`. Only allow `edit` when
  the user expects fixes, not diagnosis only.
- Builder / implementer: allow `edit` and possibly `shell` only when actual code
  changes or command execution are part of the job.

Important rules to preserve:

- `write` and `patch` are governed by the `edit` permission action.
- `websearch` is for discovery; `webfetch` is for a known URL.
- Restrict capability with `permissions` rules, not with a `tools` map; V2
  deprecated `tools` and `permission`.
- If the user wants shell access but with guardrails, prefer command-specific
  `shell` permission rules over broad unrestricted access.

## Draft The Agent File

Create a Markdown file in `.opencode/agents/`.

The filename becomes the agent name, so choose a stable, descriptive, lowercase,
shell-friendly identifier with hyphens when needed.

The frontmatter must be valid YAML. Include at least:

- `description`: required, concrete, and discoverable
- `mode`: `primary`, `subagent`, or `all`

Add other fields only when they improve the agent materially, for example:

- `permissions`: ordered `{ action, resource, effect }` rules
- `model`
- `steps`
- `hidden` for internal-only subagents
- `color`

The body prompt should:

- clearly state the role
- define the job boundary
- explain how to decide what to do first
- specify any tool restrictions or priorities
- state the output style expected from the agent

## Drafting Template

Use this as a starting shape and adapt it to the job:

```md
---
description: "Short, specific description with trigger phrases"
mode: subagent
---

You are a specialized OpenCode agent.

## Mission

State the concrete job.

## Rules

- Keep this prompt written in English.
- State any language, tool, or safety rules.

## Procedure

1. Inspect the relevant context.
2. Take the narrowest correct action.
3. Return a concise result in the requested style.
```

## Save And Review

After drafting:

1. Save the agent under `.opencode/agents/<name>.md`.
2. Check the frontmatter for validity and required fields.
3. Review whether the description is specific enough to trigger correctly.
4. Review whether mode, tools, and permissions are narrower than the maximum
   possible set.
5. Identify the weakest assumption and ask the user about it in Simplified
   Chinese if it could materially change the agent.

## Iterate

If the user refines the role, update the draft instead of rewriting from scratch
unless the original mode or safety posture is fundamentally wrong.

If the user asks for a rename or trigger improvement, prefer the smallest edit
set that fixes discovery and keeps the existing workflow intact.

## Examples

```text
Create a read-only reviewer subagent that reports P0/P1 findings.
```

```text
Tighten the existing release agent's description and permissions.
```

## Troubleshooting

- The agent never gets picked: make `description` concrete and add
  near-miss task phrases; a vague description is the usual cause.
- The frontmatter is invalid: `description` and `mode` are required, and
  an unquoted colon in a description is the usual YAML error.
- The agent is missing from the picker: check `mode` and that the file
  sits directly in `.opencode/agents/` with valid frontmatter.
- The agent edits files it should not: replace `tools` overrides with
  ordered `permissions` rules and re-check the narrowest capability set.

## Limitations

- Agent discovery depends on the harness version; verify a new file
  appears before promising behavior.
- This skill writes one file at a time and does not migrate agents out of
  user-global configuration.
- Model choice is a per-agent field, not a project-wide default.

## Final Response

When the draft is ready, summarize in Simplified Chinese:

- what the agent does
- which mode it uses and why
- the main tools or permissions it has
- one or two example prompts the user can try
- one or two related customizations worth creating next, only if useful
