---
title: Accounts and authentication - ZZZ
description: ZZZ does not require an account; configure credentials only for providers you choose.
---

# Accounts and authentication

ZZZ has no default account, sign-in, subscription, or billing flow. Editing,
local AI providers, Git, LSP, MCP, and external agents work without a ZZZ
account.

When you explicitly configure a remote provider, its API key is stored in your
operating system credential store and is sent only to that provider. If you
configure a remote collaboration server, authentication is handled by that
server. Leaving `server_url` at the local default keeps cloud compatibility
disabled.
