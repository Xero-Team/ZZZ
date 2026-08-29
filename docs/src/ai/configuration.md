---
title: Configure AI in ZZZ - Providers, Models, and Settings
description: Set up local-first AI in ZZZ with your own API keys or external agents. Includes how to disable AI entirely.
---

# Configuration

You can configure multiple dimensions of AI usage in ZZZ:

1. Which LLM providers you can use
   - Local Ollama or llama.cpp models (the default preference)
   - [Using your own API keys](./llm-providers.md)
   - [External agents like Claude Agent](./external-agents.md)
   - A remote provider configured manually by you
2. [Model parameters and usage](./agent-settings.md#model-settings)
3. [Interactions with the Agent Panel](./agent-settings.md#agent-panel-settings)

## Turning AI Off Entirely

To disable all AI features, add the following to your settings file ([how to edit](../configuring-zed.md#settings-files)):

```json [settings]
{
  "disable_ai": true
}
```

This setting is local and takes effect without contacting any service.
