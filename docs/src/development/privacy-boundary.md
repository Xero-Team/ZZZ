---
title: Privacy and Network Boundary
description: Default network behavior and explicit opt-in rules for ZZZ.
---

# Privacy and Network Boundary

ZZZ is local-first. A fresh installation does not create an account, contact a
hosted ZZZ service, send telemetry, upload crash data, or install extensions
from the network.

## Allowed by default

- Loopback connections used by local Ollama (`localhost:11434`), llama.cpp
  (`localhost:8080`), LM Studio, and user-configured local tools.
- User-initiated Git, LSP, MCP, and browser actions.
- User-initiated Extension Gallery browse, install, and upgrade requests to the
  public Zed marketplace (`https://api.zed.dev`).
- Providers configured explicitly by the user, including OpenAI-compatible
  endpoints.

## Disabled by default

- Hosted collaboration, cloud AI, account, subscription, billing, and trial
  requests.
- Telemetry, Anthropic behavior logging, Sentry uploads, and crash reporting.
- Automatic application updates.
- Automatic extension installation or updates.

## Explicit opt-in

Set a remote `server_url` manually to enable cloud compatibility. OpenCode is
available only when configured by the user. Sentry symbol uploads additionally
require `ZZZ_ENABLE_SENTRY_UPLOAD=1` and a valid `SENTRY_AUTH_TOKEN`.
