---
title: Uninstall
description: "This guide covers how to uninstall ZZZ on different operating systems."
---

# Uninstall

This guide covers how to uninstall ZZZ on different operating systems.

## macOS

### Standard Installation

If you installed ZZZ by downloading it from the website:

1. Quit ZZZ if it's running
2. Open Finder and go to your Applications folder
3. Drag ZZZ to the Trash (or right-click and select "Move to Trash")
4. Empty the Trash

### Homebrew Installation

If you installed ZZZ using Homebrew, use the following command:

```sh
brew uninstall --cask zed
```

### Removing User Data (Optional)

To completely remove all ZZZ configuration files and data:

1. Open Finder
2. Press `Cmd + Shift + G` to open "Go to Folder"
3. Delete the following directories if they exist:
   - `~/Library/Application Support/ZZZ`
   - `~/Library/Saved Application State/dev.zzz.ZZZ.savedState`
   - `~/Library/Logs/ZZZ`
   - `~/Library/Caches/dev.zed.Zed`
   - `~/Library/Caches/ZZZ`
   - `~/.config/ZZZ`
   - `~/.local/state/ZZZ`

## Linux

### Standard Uninstall

If ZZZ was installed using the default installation script, run:

```sh
zzz --uninstall
```

You'll be prompted whether to keep or delete your preferences. After making a choice, you should see a message that ZZZ was successfully uninstalled.

If the `zzz` command is not found in your PATH, try:

```sh
$HOME/.local/bin/zzz --uninstall
```

or:

```sh
$HOME/.local/zzz.app/bin/zzz --uninstall
```

### Package Manager

If you installed ZZZ using a package manager (such as Flatpak, Snap, or a distribution-specific package manager), consult that package manager's documentation for uninstallation instructions.

### Manual Removal

If the uninstall command fails or ZZZ was installed to a custom location, you can manually remove:

- Installation directory: `~/.local/zzz.app` (or your custom installation path)
- Binary symlink: `~/.local/bin/zzz`
- Configuration and data: `~/.config/ZZZ`

## Windows

### Standard Installation

1. Quit ZZZ if it's running
2. Open Settings (Windows key + I)
3. Go to "Apps" > "Installed apps" (or "Apps & features" on Windows 10)
4. Search for "ZZZ"
5. Click the three dots menu next to ZZZ and select "Uninstall"
6. Follow the prompts to complete the uninstallation

Alternatively, you can:

1. Open the Start menu
2. Right-click on ZZZ
3. Select "Uninstall"

### Removing User Data (Optional)

To completely remove all ZZZ configuration files and data:

1. Press `Windows key + R` to open Run
2. Type `%APPDATA%` and press Enter
3. Delete the `ZZZ` folder if it exists
4. Press `Windows key + R` again, type `%LOCALAPPDATA%` and press Enter
5. Delete the `ZZZ` folder if it exists

## Troubleshooting

If you encounter issues during uninstallation:

- **macOS/Windows**: Ensure ZZZ is completely quit before attempting to uninstall. Check Activity Manager (macOS) or Task Manager (Windows) for any running ZZZ processes.
- **Linux**: If the uninstall script fails, check the error message and consider manual removal of the directories listed above.
- **All platforms**: If you want to start fresh while keeping ZZZ installed, you can delete the configuration directories instead of uninstalling the application entirely.

For additional help, see our [Linux-specific documentation](./linux.md) or visit the [ZZZ community](https://zed.dev/community-links).
