---
title: CLI Reference
description: "Reference for ZZZ's command-line interface (CLI), including opening files and directories, integrating with tools, and controlling ZZZ from scripts."
---

# CLI Reference

Use ZZZ's command-line interface (CLI) to open files and directories, integrate with other tools, and control ZZZ from scripts.

## Installation

**macOS:** Run the `cli: install` command from the command palette ({#kb command_palette::Toggle}) to install the `/usr/local/bin/zzz` CLI to `/usr/local/bin/zzz`.

**Linux:** The CLI is included with ZZZ packages. The binary name may vary by distribution (commonly `zzz`, `zedit`, or `zeditor`).

**Windows:** The CLI is included with ZZZ. Add ZZZ's installation directory to your PATH, or use the full path to `zzz.exe`.

## Usage

```sh
zzz [OPTIONS] [PATHS]...
```

## Opening Files and Directories

Open a file:

```sh
zzz myfile.txt
```

Open a directory as a workspace:

```sh
zzz ~/projects/myproject
```

Open multiple files or directories:

```sh
zzz file1.txt file2.txt ~/projects/myproject
```

Open a file at a specific line and column:

```sh
zzz myfile.txt:42        # Open at line 42
zzz myfile.txt:42:10     # Open at line 42, column 10
```

## Options

### `-w`, `--wait`

Wait for all opened files to be closed before the CLI exits. When opening a directory, waits until the window is closed.

This is useful for integrating ZZZ with tools that expect an editor to block until editing is complete (e.g., `git commit`):

```sh
export EDITOR="zzz --wait"
git commit  # Opens ZZZ and waits for you to close the commit message file
```

### `-n`, `--new`

Open paths in a new workspace window, even if the paths are already open in an existing window:

```sh
zzz -n ~/projects/myproject
```

### `-a`, `--add`

Add paths to the currently focused workspace instead of opening a new window. When multiple workspace windows are open, files open in the focused window:

```sh
zzz -a newfile.txt
```

### `-r`, `--reuse`

Reuse an existing window, replacing its current workspace with the new paths:

```sh
zzz -r ~/projects/different-project
```

### `-e`, `--existing`

Open paths in an existing Zed window instead of creating a new one:

```sh
zed -e myfile.txt
```

By default (without `-n`, `-a`, `-r`, or `-e`), directories open in the current window's sidebar. You can change this default with the `cli_default_open_behavior` setting. See [Windows & Projects](../windows-and-projects.md) for more details.

### `--diff <OLD_PATH> <NEW_PATH>`

Open a diff view comparing two files. Can be specified multiple times:

```sh
zzz --diff file1.txt file2.txt
zzz --diff old.rs new.rs --diff old2.rs new2.rs
```

### `--foreground`

Run ZZZ in the foreground, keeping the terminal attached. Useful for debugging:

```sh
zzz --foreground
```

### `--user-data-dir <DIR>`

Use a custom directory for all user data (database, extensions, logs) instead of the default location:

```sh
zzz --user-data-dir ~/.zzz-custom
```

Default locations:

- **macOS:** `~/Library/Application Support/ZZZ`
- **Linux:** `~/.local/share/zzz` (typically `~/.local/share/zzz`)
- **Windows:** `%LOCALAPPDATA%\ZZZ`

### `-v`, `--version`

Print ZZZ's version and exit:

```sh
zzz --version
```

### `--completions <SHELL>`

Generate shell completions for the `zzz` CLI:

#### Bash

Add to `~/.bashrc`:

```bash
eval "$(zzz --completions bash)"
```

#### Elvish

Add to `~/.config/elvish/rc.elv`:

```elvish
set edit:completion:arg-completer[zzz] = { |@args|
    eval (zzz --completions elvish | slurp)
    $edit:completion:arg-completer[zzz] $@args
}
```

#### Fish

Add to `~/.config/fish/config.fish`:

```fish
zzz --completions fish | source
```

#### Nushell

Add to `~/.config/nushell/config.nu`:

```nu
mkdir ($nu.data-dir | path join "vendor/autoload")
^zzz --completions nushell | save --force ($nu.data-dir | path join "vendor/autoload/zzz.nu")
```

#### Powershell

Add to `$PROFILE`:

```powershell
(&zzz --completions powershell) | Out-String | Invoke-Expression
```

#### Zsh

Add to `~/.zshrc`:

```zsh
eval "$(zzz --completions zsh)"
```

### `--uninstall`

Uninstall ZZZ and remove all related files (macOS and Linux only):

```sh
zzz --uninstall
```

### `--zzz <PATH>`

Specify a custom path to the ZZZ application or binary:

```sh
zzz --zzz /path/to/ZZZ.app myfile.txt
```

## Reading from Standard Input

Read content from stdin by passing `-` as the path:

```sh
echo "Hello, World!" | zzz -
cat myfile.txt | zzz -
ps aux | zzz -
```

This creates a temporary file with the stdin content and opens it in ZZZ.

## URL Handling

The CLI can open `zzz://`, `file://`, and `ssh://` URLs:

```sh
zzz zzz://settings
zzz file:///Users/whatever/.zshrc
zzz ssh://me@example.com/abs/path
zzz ssh://me@example.com:/abs/path
zzz ssh://me@example.com/~/project
zzz ssh://me@example.com:~/project
```

## Using ZZZ as Your Default Editor

Set ZZZ as your default editor for Git and other tools:

```sh
export EDITOR="zzz --wait"
export VISUAL="zzz --wait"
```

Add these lines to your shell configuration file (e.g., `~/.bashrc`, `~/.zshrc`).

## macOS: Switching Installed Channels

On macOS, you can launch a specific installed channel by passing the channel name as the first argument:

```sh
zzz --stable myfile.txt
zzz --dev myfile.txt
```

Legacy `--preview` and `--nightly` channel selectors are no longer supported.

## WSL Integration (Windows)

On Windows, the CLI supports opening paths from WSL distributions. This is handled automatically when launching ZZZ from within WSL.

## Exit Codes

| Code | Meaning                           |
| ---- | --------------------------------- |
| `0`  | Success                           |
| `1`  | Error (details printed to stderr) |

When using `--wait`, the exit code reflects whether the files were saved before closing.
