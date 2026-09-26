---
title: Edit Prediction (Retired)
description: Edit prediction has been retired in favor of external ACP agents.
---

# Edit Prediction

Edit Prediction has been retired in ZZZ. The native edit prediction providers
and the in-editor suggestion experience were removed. AI assistance now comes
from external agents that ZZZ connects to over the Agent Client Protocol (ACP).

Use the [Agent Panel](./agent-panel.md) to run an agent such as Claude Code,
Gemini CLI, or OpenCode. See [External Agents](./external-agents.md) for setup.

## Using GitHub Copilot Enterprise

GitHub Copilot is still available as an LLM provider for the agent and inline
assistant. If your organization uses GitHub Copilot Enterprise, specify the
enterprise URI in your settings file
([how to edit](../configuring-zzz.md#settings-files)):

```json [settings]
{
  "edit_predictions": {
    "copilot": {
      "enterprise_uri": "https://your.enterprise.domain"
    }
  }
}
```

Replace `"https://your.enterprise.domain"` with the URL provided by your GitHub
Enterprise administrator (e.g., `https://foo.ghe.com`).

Once set, ZZZ routes Copilot requests through your enterprise endpoint.
