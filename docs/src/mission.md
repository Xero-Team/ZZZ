---
title: The Mission
description: "Why ZZZ exists, what it keeps, what it removes, and how those boundaries are enforced."
---

# The Mission

ZZZ is a fork of the [Zed](https://zed.dev) editor. This document explains
why the fork exists, what it deliberately keeps, and what it deliberately
leaves out.

The short version: some things should not be configurable. They should
simply be absent.

## Why fork

Zed is a fast editor with a strong foundation. It also ships, by default,
a set of surfaces that we do not want in a text editor: product telemetry,
an account and subscription system, hosted AI, hosted collaboration, and
involuntary auto-updates. Upstream treats all of these as opt-out. We think
that is backwards.

A fork lets us remove those surfaces in the source code instead of asking
every user to hunt through settings and hope the default is honest. When a
feature is gone from the code, it cannot be re-enabled by an update, a
migration, or a misread setting.

This is a reaction, but it is not only a reaction. The goal is an editor
that is local-first by construction.

## What stays

- **AI features.** We keep the agent panel, inline assistant, and edit
  prediction. They default to your own infrastructure: a local Ollama or
  llama.cpp endpoint, or a provider you configure yourself. No account is
  created, and no remote provider is selected for you.
- **The agent protocol.** ZZZ speaks the open Agent Client Protocol (ACP)
  and nothing proprietary.
- **The foundation.** GPUI, Tree-sitter, the language server integration,
  the debugger, Vim and Helix modes, remote development over SSH, and the
  rest of the editor are unchanged.

## What is absent

- Telemetry and product analytics.
- Crash reporting. Minidumps and Sentry symbol uploads are opt-in only,
  behind explicit environment variables.
- Accounts, subscriptions, trials, billing, and upgrade prompts.
- Hosted collaboration and its server component.
- The native Zed agent and its configuration target.
- Automatic application updates.
- Hosted documentation and hosted release automation.

The exact network boundary, including what is allowed by default and what
requires an explicit opt-in, is documented in
[Privacy and Network Boundary](./development/privacy-boundary.md).

## How this differs from other forks

[Gram](https://codeberg.org/GramEditor/gram) removes AI entirely. That is a
principled position, and Gram proved that a fork built on principle rather
than preference is worth doing. [Zedless](https://codeberg.org/zedless-editor/zedless)
takes a similar privacy-first approach.

ZZZ takes a third position: AI can stay, but it points at your own
infrastructure and stays quiet. If you want a remote provider, you add it
yourself, explicitly.

## How the boundary is enforced

A written promise is not enough, so the boundary is checked mechanically:

- `script/check-philosophy` asserts that removed surfaces stay removed. It
  runs against the product defaults, the source tree, the localization
  catalogs, the legal documents, and these docs.
- Upstream changes are absorbed selectively. A commit that carries a
  removed surface is not merged, no matter how useful it is on its own.
  The process is described in
  [Upstream Cherry-Pick](./development/upstream-cherrypick.md).

If a change would reintroduce a removed surface, it belongs in a fork, not
in ZZZ.

## Get involved

Contributions are accepted under the Developer Certificate of Origin, with
no CLA and no copyright assignment. ZZZ requires AI-assisted review for
every change; see [CONTRIBUTING.md](../../CONTRIBUTING.md) for the policy
and the reasoning.
