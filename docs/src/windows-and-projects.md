---
title: Windows & Projects
description: "How ZZZ handles multiple projects in windows, including the threads sidebar and options for opening in new windows."
---

# Windows & Projects

ZZZ lets you work on multiple projects in a single window. Projects appear in the threads sidebar on the left, and you can switch between them while keeping your context intact.

## How Projects Open

By default, when you open a folder in ZZZ, it opens in a new window. If you'd rather keep related work together, you can open folders as projects in the current window's threads sidebar instead.

| Action             | Result                |
| ------------------ | --------------------- |
| File > Open        | Opens in a new window |
| File > Open Recent | Opens in a new window |
| Drag folder to ZZZ | Opens in a new window |
| `zzz ~/project`    | Opens in a new window |

## Working with Multiple Projects

When you have multiple projects open in one window's threads sidebar:

- Click a project header to collapse or expand its threads; Cmd+click (macOS) or Ctrl+click (Linux/Windows) to switch to that project
- Each project has its own file tree, git state, and search scope
- Agent threads are tied to their project context
- Your workspace layout (splits, tabs) is preserved per project

Think of projects in the threads sidebar like browser tabs, but for repositories.

## Opening in the Current Window

To add a folder to the window you already have open instead of creating a new one:

### From Open Recent

When using File > Open Recent ({#kb projects::OpenRecent}):

- **Enter** or **click** opens in a new window
- **Cmd+Enter** or **Cmd+click** (macOS) / **Ctrl+Enter** or **Ctrl+click** (Linux/Windows) opens in the current window's threads sidebar

### From the CLI

Use the `-a` flag to add a folder to the current window's threads sidebar:

```sh
zzz -a ~/projects/other-project
```

Other CLI options for controlling window behavior:

| Flag               | Behavior                                           |
| ------------------ | -------------------------------------------------- |
| `-n`, `--new`      | Always open in a new window                        |
| `-a`, `--add`      | Add to the current window's threads sidebar        |
| `-e`, `--existing` | Open files in an existing window                   |
| `-r`, `--reuse`    | Replace the current project in the existing window |

See [CLI Reference](./reference/cli.md) for full details.

### Via Settings

Two settings control the default:

- `cli_default_open_behavior` (default `new_window`): how `zzz <path>` opens directories.
- `default_open_behavior` (default `new_window`): how File > Open and Open Recent open projects.

```json [settings]
{
  "cli_default_open_behavior": "new_window",
  "default_open_behavior": "new_window"
}
```

For both, `new_window` opens in a new window and `existing_window` opens in the current window's threads sidebar.

## Adding Folders to a Project

If you want to add a folder to your current project (not as a separate project in the threads sidebar), you have several options:

- **File menu**: File > Add Folder to Project
- **Project panel**: Right-click in the project panel and choose "Add Folders to Project"
- **Open Recent**: Select a recent project and click the "Add Folder to this Project" button

This adds the folder as an additional root in your current project's file tree, similar to VS Code's multi-root workspaces.

## See Also

- [Threads Sidebar](./ai/parallel-agents.md#threads-sidebar): Managing threads across projects
- [Getting Started](./getting-started.md): Essential commands and setup
- [VS Code Migration](./migrate/vs-code.md): How ZZZ's project model differs from VS Code
