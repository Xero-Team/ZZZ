---
title: AI Code Editor Documentation - ZZZ
description: Docs for AI in ZZZ. Local Ollama and llama.cpp first, then optional remote providers, agentic coding, inline edits, and completions.
---

# AI

ZZZ is an open-source code editor with optional AI features. Agents can read
and write your code, transform selections inline, and talk to a model you
configure. Completions run only after you choose a provider.

## How ZZZ approaches AI

ZZZ's AI features run inside a native, GPU-accelerated application built in
Rust. There is no Electron layer between you and the model output.

- **Local first.** Point the agent at Ollama (`localhost:11434`) or
  llama.cpp (`localhost:8080`). Remote APIs are silent until you add them.
- **Open source.** The editor and all AI features are available in this
  repository. You can inspect how AI is implemented, how data flows to
  providers, and how tool calls execute.
- **Multi-model.** [Bring your own API keys](./llm-providers.md) or run
  local models. Remote APIs are explicit opt-in choices.
- **External agents.** Run Claude Agent, Gemini CLI, Codex, and other
  CLI-based agents directly in ZZZ through the Agent Client Protocol. See
  [External Agents](./external-agents.md).
- **Privacy by default.** ZZZ does not collect training data. Requests go
  to the provider you configure. See
  [Privacy and Security](./privacy-and-security.md).

## Agentic editing

The [Threads Sidebar](./parallel-agents.md#threads-sidebar) is where you organize agent work. Start a thread, give it a task, and the agent reads, edits, and runs code in your project. You can also open terminal threads directly in the sidebar alongside your agent threads. Run multiple agent threads and terminal threads at once, each using a different agent and working against different projects. See [Tools](./tools.md) for the capabilities available to ZZZ's built-in agent.

The [Agent Panel](./agent-panel.md) is the conversation view for the active thread. Use it to send prompts, review changes, add context, and interact with the agent as it works.

You can extend agents with additional tools through [MCP servers](./mcp.md), control what they can access with [tool permissions](./tool-permissions.md), and shape their behavior with [rules](./rules.md).

The [Inline Assistant](./inline-assistant.md) works differently: select code or a terminal command, describe what you want, and the model rewrites the selection in place. It works with multiple cursors.

## Code completions

[Edit Prediction](./edit-prediction.md) provides AI code completions on every keystroke. Each keypress sends a request to the prediction provider, which returns single or multi-line suggestions you accept with `tab`.

There is no default hosted prediction model. Prefer a local provider such
as Ollama or llama.cpp. GitHub Copilot and Codestral are optional and only
used after you select them.

## Getting started

- [Configuration](./configuration.md): Connect to Anthropic, OpenAI, Ollama, Google AI, or other LLM providers.
- [Parallel Agents](./parallel-agents.md): Run multiple threads at once with the Threads Sidebar.
- [External Agents](./external-agents.md): Run Claude Agent, Codex, Aider, or other external agents inside ZZZ.
- [Models](./models.md): Choose models from local or explicitly configured providers.
- [Privacy and Security](./privacy-and-security.md): How ZZZ handles data when using AI features.

New to ZZZ? Start with [Getting Started](../getting-started.md), then come back here to set up AI.
