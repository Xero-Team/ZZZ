---
title: Configure AI in ZZZ - External Agents and Settings
description: Set up AI in ZZZ with external ACP agents. Includes how to disable AI entirely.
---

# Configuration

You can configure multiple dimensions of AI usage in ZZZ:

1. [External agents like Claude Agent](./external-agents.md) that ZZZ connects
   to over the Agent Client Protocol.
2. [Agent interactions](./agent-settings.md#agent-panel-settings)
3. [Models](./models.md), selected through your agent.

## Turning AI Off Entirely

To disable all AI features, add the following to your settings file ([how to edit](../configuring-zzz.md#settings-files)):

```json [settings]
{
  "disable_ai": true
}
```

This setting is local and takes effect without contacting any service.
