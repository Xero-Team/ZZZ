---
title: Upstream Cherry-Pick
description: How OpenCode absorbs Zed upstream commits into ZZZ.
---

# Upstream Cherry-Pick

ZZZ tracks Zed `main` by selective absorption, not by merge or
fast-forward. Things that violate the fork philosophy stay absent.

OpenCode owns this workflow. Do not paste a Codex prompt.

## Invoke

In OpenCode:

- Switch to the `absorbing-upstream` agent, or
- Run `/absorbing-upstream`, optionally with a batch override.

Examples:

```text
/absorbing-upstream first 20 commits
```

```text
吸收上游，从 027cf0def7 之后开始，先处理 20 个 commit
```

The agent loads
`.agents/skills/absorbing-upstream/SKILL.md` and follows
`.agents/skills/absorbing-upstream/REFERENCE.md`.

Restart OpenCode after pulling these files. Running sessions keep the
already-loaded config.

## Current Baseline

As of 2026-08-18:

- Local `main`: `7a3be67b422d2cfe8550f9d7c633ae9f81da77ab`
- Last audit:
  [Upstream Sync 2026-08-07](./upstream-sync-2026-08-07.md)
- Last reviewed upstream: `027cf0def75e5c027504f402a6a6c0dcac11f178`
- Live upstream `main` at prompt time:
  `aa3718614b3ade75524be6f8b2e101bd1166e02c`

The next range starts after the last reviewed SHA. Query live
`https://github.com/zed-industries/zed.git` `refs/heads/main` at run
start. Do not claim commits you did not classify.

## What Lands

| Class | Meaning                                                        |
| ----- | -------------------------------------------------------------- |
| A     | Complete safe absorption, or already-equivalent local behavior |
| B     | Manual port of only the isolatable, philosophy-safe behavior   |
| C     | Rejected. No product code change                               |

A uses `git cherry-pick -x -s`. B uses `git commit -s` with
`Upstream` / `Retained` / `Omitted`. C leaves the tree clean.

Hard boundary: local-first, no-account, ACP-only. Telemetry, sign-in,
billing, native Zed Agent, collaboration, and unused public APIs stay
out. See `README.md` and the skill `REFERENCE.md`.

## Reports

Each run appends or creates
`docs/src/development/upstream-sync-YYYY-MM-DD.md`.
