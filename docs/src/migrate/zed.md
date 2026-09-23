---
title: How to Migrate from Zed to ZZZ
description: "Guide for moving from Zed to ZZZ: configuration, removed surfaces, and changed defaults."
---

# How to Migrate from Zed to ZZZ

ZZZ is a fork of Zed. This guide covers what carries over, what is gone,
and what behaves differently so you can move without surprises.

If you are coming from another editor, start with the
[VS Code](./vs-code.md) guide instead.

## Install ZZZ

ZZZ does not publish pre-built binaries yet. Build from source:

```sh
cargo run
```

See [Installation](../installation.md) and the platform guides for
[macOS](../development/macos.md), [Linux](../development/linux.md), and
[Windows](../development/windows.md).

## Move your configuration

ZZZ keeps its configuration under the `zzz` directory, not `zed`:

| Platform | Zed              | ZZZ              |
| -------- | ---------------- | ---------------- |
| macOS    | `~/.config/zed/` | `~/.config/zzz/` |
| Linux    | `~/.config/zed/` | `~/.config/zzz/` |
| Windows  | `%AppData%\Zed\` | `%AppData%\ZZZ\` |

Copy `settings.json` and `keymap.json` from the Zed directory into the ZZZ
directory. The format is unchanged, so most settings and keybindings work
as-is.

Some settings that only exist upstream are ignored. Settings that control a
removed surface, such as auto-update or a hosted provider, have no effect.

## What is removed

These surfaces do not exist in ZZZ and cannot be enabled:

- Product telemetry and analytics.
- Crash reporting. Minidumps and Sentry symbol uploads are opt-in, behind
  explicit environment variables.
- Accounts, sign-in, subscriptions, trials, and billing.
- Hosted collaboration and the proprietary server component.
- The native Zed agent. ZZZ speaks the open Agent Client Protocol (ACP)
  instead.
- Automatic application updates.
- Hosted documentation and hosted release automation.

The full network boundary is documented in
[Privacy and Network Boundary](../development/privacy-boundary.md).

## What behaves differently

- **AI providers default to local.** The editor ships pointed at a local
  endpoint. No remote provider is selected for you, and no account is
  created. Add a remote provider yourself if you want one. See
  [AI Configuration](../ai/configuration.md).
- **Agent protocol.** ZZZ uses ACP. Agent servers that speak ACP work;
  anything that expects the native Zed agent does not.
- **Extension Gallery.** Browsing, installing, and updating extensions use
  the public Zed marketplace. Worktree trust and extension capabilities
  still apply; see [Worktree Trust](../worktree-trust.md) and
  [Extension Capabilities](../extensions/capabilities.md).
- **Updates.** ZZZ does not update itself. You rebuild or reinstall when
  you choose.

## Keybindings and settings

The default keymap is close to Zed's. Vim and Helix modes, multibuffers,
tasks, and the debugger all behave the same. If a shortcut feels wrong,
compare the [Keybindings](../key-bindings.md) page and adjust your
`keymap.json`.

## Getting help

If something works in Zed but not in ZZZ, open an issue:

<https://github.com/Xero-Team/ZZZ/issues/new>

Describe the Zed behavior, the ZZZ behavior, and the steps to reproduce.
