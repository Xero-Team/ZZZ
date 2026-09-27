---
title: Models in ZZZ
description: Choose a model through the external agent you connect in ZZZ.
---

# Models

ZZZ does not ship a hosted model catalog, usage plan, or its own model runtime.
Model availability and limits come from the external agent you connect over the
Agent Client Protocol and whatever model provider that agent uses.

## Discovering models

Open the model selector in the [Agent Panel](./agent-panel.md) toolbar. ZZZ
lists the models the connected agent reports. No model catalog is fetched during
startup.

## Selecting a model

Choose a model from the Agent Panel model selector. Some agents also accept a
default model as a launch argument or environment variable; see
[External Agents](./external-agents.md).

## Provider limits

Context windows, rate limits, billing, and retention policies are controlled by
the agent and its model provider. ZZZ itself does not meter usage, manage
accounts, or charge for model access.
