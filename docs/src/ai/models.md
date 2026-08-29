---
title: Models in ZZZ
description: Choose and configure models from local or explicitly configured providers in ZZZ.
---

# Models

ZZZ does not ship a hosted model catalog or usage plan. Model availability and
limits come from the provider you configure. Local Ollama models are preferred,
followed by llama.cpp; OpenCode and other remote providers are opt-in.

## Discovering models

Open the Agent Panel settings to inspect models reported by a provider. Ollama
and llama.cpp discovery runs against the local endpoint you configured. No model
catalog is fetched during startup.

## Selecting a model

Set an explicit model in `settings.json` when you need a stable choice:

```json [settings]
{
  "agent": {
    "default_model": {
      "provider": "ollama",
      "model": "your-installed-model"
    }
  }
}
```

If no model is selected, ZZZ chooses the first available discovered Ollama
model, then llama.cpp. A manually configured OpenCode provider is considered
only after those local providers.

## Provider limits

Context windows, rate limits, billing, and retention policies are controlled by
the provider. ZZZ itself does not meter usage, manage accounts, or charge for
model access.
