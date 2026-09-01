---
title: AI Privacy and Security - ZZZ
description: "ZZZ's local-first AI privacy: no default collection, no hosted training program, and explicit provider configuration."
---

# Privacy and Security

## Philosophy

ZZZ is local-first. It does not collect product-improvement data, telemetry,
or AI training samples by default.

- **AI**: Requests go to the provider you configure. Local Ollama and
  llama.cpp stay on the host you point at. Remote APIs are explicit
  opt-in. ZZZ does not retain prompts or code on a hosted service.
- **Open-Source**: The codebase is public. You can inspect how requests
  leave the editor and which settings control them.
- **No account**: ZZZ does not require an account or subscription.

## Related Documentation

- [Tool Permissions](./tool-permissions.md): Configure granular rules to
  control which agent actions are auto-approved, blocked, or require
  confirmation.
- [Worktree trust](../worktree-trust.md): How ZZZ opens files and
  directories in restricted mode.
- [Privacy boundary](../development/privacy-boundary.md): Default network
  behavior and explicit opt-ins.

ZZZ does not require an account or subscription. See the repository's
local privacy and contribution documents for project policies.
