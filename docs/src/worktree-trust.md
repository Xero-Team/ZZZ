---
title: ZZZ and trusted worktrees
description: "Configure which folders ZZZ trusts for running code and extensions."
---

# ZZZ and trusted worktrees

A worktree in ZZZ is either a directory or a single file that ZZZ opens as a standalone "project".
ZZZ opens a worktree each time you run `zzz some/path`, drag a file or directory into ZZZ, or open your user settings file.

> Note: This is broader than a [Git worktree](./git.md#git-worktrees). A Git worktree is a linked checkout managed by Git; ZZZ's trust model applies to every opened file or folder root, including Git worktrees.

Every worktree opened may contain a `.ZZZ/settings.json` file with extra configuration options that may require installing and spawning language servers or MCP servers.
To let users choose based on their own threat model and risk tolerance, all worktrees start in Restricted Mode. Restricted Mode prevents downloading and running related items from `.ZZZ/settings.json`. Until a worktree is trusted, ZZZ does not run related untrusted actions and waits for user confirmation. This gives users a chance to review project settings, MCP servers, and language servers.

ZZZ still trusts tools it installs globally. Global MCP servers and global
language servers such as Prettier are installed and started as usual,
independent of worktree trust. Copilot is not installed by default; it
starts only after you select it as a provider.

If a worktree is not trusted, ZZZ will indicate this with an exclamation mark icon in the title bar. Clicking this icon or using `workspace::ToggleWorktreeSecurity` action will bring up the security modal that allows the user to trust the worktree.

Trusting a worktree persists that decision between restarts. You can clear all trusted worktrees with the `workspace::ClearTrustedWorktrees` command.
This command will restart ZZZ, to ensure no untrusted settings, language servers or MCP servers persist.

This feature works locally and on SSH and WSL remote hosts. ZZZ tracks trust information per host in these cases.

## What is restricted

Restricted Mode prevents:

- Project settings (`.ZZZ/settings.json`) from being parsed and applied
- Language servers from being installed and spawned
- MCP servers from being installed and spawned

## Configuring broad worktree trust

By default, ZZZ does not trust new worktrees. Users must trust each new worktree individually. Though not recommended, users can trust all worktrees with this setting ([how to edit](./configuring-zzz.md#settings-files)):

```json [settings]
"session": {
  "trust_all_worktrees": true
}
```

Auto-trusted worktrees are not persisted between restarts; only manually trusted worktrees are. This ensures users make new trust decisions if they later disable `trust_all_worktrees`.

## Trust hierarchy

These are mostly internal details and may change, but they help explain how multiple trust requests can be approved at once.
ZZZ has multiple layers of trust, based on the requests, from the least to most trusted level:

- "single file worktree"

After opening an empty ZZZ window, you can open a single file. You can also open a file outside the current directory after opening a directory.
A common example is `zzz: open settings file`, which may start a language server for that file and create a new single-file worktree.

Spawning a language server presents a risk should the language server experience a supply-chain attack; therefore, ZZZ restricts that by default. Each single file worktree requires a separate trust grant, unless the directory containing it is trusted or all worktrees are trusted.

- "directory worktree"

If a directory is open in ZZZ, it is a full worktree. It may spawn multiple language servers and MCP servers defined in project settings. Each directory worktree therefore requires a separate trust grant unless a parent-directory trust grant exists (see below).

When a directory worktree is trusted, language and MCP servers are permitted to be downloaded and started, hence we also enable single file worktree trust for the host in question automatically when this occurs: this helps when opening single files when using language server features in the trusted directory worktree.

- "parent directory worktree"

To permit trust decisions for multiple directory worktrees at once, it's possible to trust all subdirectories of a given parent directory worktree opened in ZZZ by checking the appropriate checkbox. This will grant trust to all its subdirectories, including all current and potential directory worktrees.
