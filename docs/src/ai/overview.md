---
title: AI Code Editor Documentation - ZZZ
description: Docs for AI in ZZZ. Connect external ACP agents for agentic coding in ZZZ.
---

# AI

ZZZ is an open-source code editor with optional AI features. AI runs through
external agents that ZZZ connects to over the Agent Client Protocol (ACP).
Agents can read and write your code, run commands, and talk to a model you
configure in that agent.

## How ZZZ approaches AI

ZZZ's AI features run inside a native, GPU-accelerated application built in
Rust. There is no Electron layer between you and the agent output.

- **External agents.** Run Claude Agent, Gemini CLI, Codex, and other
  CLI-based agents directly in ZZZ through the Agent Client Protocol. See
  [External Agents](./external-agents.md).
- **Open source.** The editor and the ACP client are available in this
  repository. You can inspect how agents are launched and how tool calls
  execute.
- **Privacy by default.** ZZZ does not collect training data. Requests go to
  the agent and model you configure. See
  [Privacy and Security](./privacy-and-security.md).

## Agentic editing

The [Threads Sidebar](./parallel-agents.md#threads-sidebar) is where you organize agent work. Start a thread, give it a task, and the agent reads, edits, and runs code in your project. You can also open terminal threads directly in the sidebar alongside your agent threads. Run multiple agent threads and terminal threads at once, each using a different agent and working against different projects. See [Tools](./tools.md) for tool permissions that apply to ACP agents.

The [Agent Panel](./agent-panel.md) is the conversation view for the active thread. Use it to send prompts, review changes, add context, and interact with the agent as it works.

You can extend agents with additional tools through [MCP servers](./mcp.md), control what they can access with [tool permissions](./tool-permissions.md), and shape their behavior with [rules](./rules.md).

## Getting started

- [Configuration](./configuration.md): Connect an external ACP agent.
- [Parallel Agents](./parallel-agents.md): Run multiple threads at once with the Threads Sidebar.
- [External Agents](./external-agents.md): Run Claude Agent, Codex, Aider, or other external agents inside ZZZ.
- [Models](./models.md): Choose a model through your agent.
- [Privacy and Security](./privacy-and-security.md): How ZZZ handles data when using AI features.

New to ZZZ? Start with [Getting Started](../getting-started.md), then come back here to set up AI.
