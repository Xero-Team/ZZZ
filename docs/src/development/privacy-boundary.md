---
title: Privacy and Network Boundary
description: Default network behavior and explicit opt-in rules for ZZZ.
---

# Privacy and Network Boundary

ZZZ is local-first. A fresh installation does not create an account, contact a
hosted ZZZ service, send telemetry, or upload crash data.

## Allowed by default

- Loopback connections used by local Ollama (`localhost:11434`), llama.cpp
  (`localhost:8080`), LM Studio, and user-configured local tools.
- Language-server, debug adapter, Prettier, and Node binary downloads needed
  to start editing.
- Extension Gallery browse, install, auto-install, and auto-update requests to
  the public Zed marketplace (`https://api.zed.dev`). The HTML extension is
  installed on startup unless you disable it.
- User-initiated Git, LSP, MCP, and browser actions.
- Author avatars in git views (inline blame, the git graph, commit tooltips,
  and the git panel). These are fetched from the hosting provider's avatar
  service, which discloses the commit author's email address to it. Disable
  them with the `git.show_avatar` setting to keep author emails local.
- Providers configured explicitly by the user, including OpenAI-compatible
  endpoints.

## Disabled by default

- Hosted collaboration, cloud AI, account, subscription, billing, and trial
  requests.
- Telemetry, Anthropic behavior logging, Sentry uploads, and crash reporting.
- Automatic application updates.
- Automatic Agent Registry downloads.

## Explicit opt-in

Set a remote `server_url` manually to enable cloud compatibility. OpenCode is
available only when configured by the user. Sentry symbol uploads additionally
require `ZZZ_ENABLE_SENTRY_UPLOAD=1` and a valid `SENTRY_AUTH_TOKEN`.
