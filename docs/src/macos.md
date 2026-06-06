---
title: ZZZ on macOS
description: "ZZZ is developed primarily on macOS, making it a first-class platform with full feature support."
---

# ZZZ on macOS

ZZZ is developed primarily on macOS, making it a first-class platform with full feature support.

## Installing ZZZ

Download ZZZ from the [download page](https://zed.dev/download). The download is a `.dmg` file—open it and drag ZZZ to your Applications folder.

After installation, ZZZ checks for updates automatically and prompts you when a new version is available.

### Homebrew

You can also install ZZZ using Homebrew:

```sh
brew install --cask zed
```

### Building from Source

To build ZZZ from source, see the [macOS development documentation](./development/macos.md).

## System Requirements

- macOS 10.15.7 (Catalina) or later
- Apple Silicon (M1/M2/M3/M4) or Intel processor

ZZZ uses Metal for GPU-accelerated rendering, which is available on all supported macOS versions.

## Installing the CLI

ZZZ includes a command-line tool for opening files and projects from Terminal. To install it:

1. Open ZZZ
2. Open the command palette with `Cmd+Shift+P`
3. Run `cli: install`

This creates a `zzz` command in `/usr/local/bin`. You can then open files and folders:

```sh
zzz .                    # Open current folder
zzz file.txt             # Open a file
zzz project/ file.txt    # Open a folder and a file
```

See the [CLI Reference](./reference/cli.md) for all available options.

## Uninstall

1. Quit ZZZ if it's running
2. Drag ZZZ from Applications to the Trash
3. Optionally, remove your settings and extensions:

```sh
rm -rf ~/.config/ZZZ
rm -rf ~/Library/Application\ Support/ZZZ
rm -rf ~/Library/Caches/ZZZ
rm -rf ~/Library/Logs/ZZZ
rm -rf ~/Library/Saved\ Application\ State/dev.zzz.ZZZ.savedState
```

If you installed the CLI, remove it with:

```sh
rm /usr/local/bin/zzz
```

## Troubleshooting

### ZZZ won't open or shows "damaged" warning

If macOS reports that ZZZ is damaged or can't be opened, it's likely a Gatekeeper issue. Try:

1. Right-click (or Control-click) on ZZZ in Applications
2. Select "Open" from the context menu
3. Click "Open" in the dialog that appears

This tells macOS to trust the application.

If that doesn't work, remove the quarantine attribute:

```sh
xattr -cr /Applications/ZZZ.app
```

### CLI command not found

If the `zzz` command isn't available after installation:

1. Check that `/usr/local/bin` is in your PATH
2. Try reinstalling the CLI via `cli: install` in the command palette
3. Open a new terminal window to reload your PATH

### GPU or rendering issues

ZZZ uses Metal for rendering. If you experience graphical glitches:

1. Ensure macOS is up to date
2. Restart your Mac to reset the GPU state
3. Check Activity Monitor for GPU pressure from other apps

### High memory or CPU usage

If ZZZ uses more resources than expected:

1. Check for runaway language servers in the terminal output (`zzz: open log`)
2. Try disabling extensions one by one to identify conflicts
3. For large projects, consider using [project settings](./reference/all-settings.md#file-scan-exclusions) to exclude unnecessary folders from indexing

For additional help, see the [Troubleshooting guide](./troubleshooting.md) or visit the [ZZZ Discord](https://discord.gg/zed-community).
