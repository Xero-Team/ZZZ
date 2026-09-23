---
title: ZZZ on Windows
description: "Build and install ZZZ locally on Windows."
---

# ZZZ on Windows

## Installing ZZZ

Build and install ZZZ locally using the instructions in this repository. Automatic updates are disabled by default.

There is no winget, Chocolatey, or installer package for ZZZ. Build from source with the [Windows development guide](./development/windows.md).

## Uninstall

- Installed via installer: Use `Settings` → `Apps` → `Installed apps`, search for ZZZ, and click Uninstall.
- Built from source: Remove the build output directory you created (e.g., your target/install folder).

Your settings and extensions live in your user profile. When uninstalling, you can choose to keep or remove them.

## Remote Development (SSH)

ZZZ supports remote development on Windows through both SSH and WSL. You can connect to remote servers via SSH or work with files inside WSL distributions directly from ZZZ.

For detailed instructions on setting up and using remote development features, including SSH configuration, WSL setup, and troubleshooting, see the [Remote Development documentation](./remote-development.md).

## Troubleshooting

### ZZZ fails to start or shows a blank window

- Check that your hardware and operating system version are compatible with ZZZ. See our [installation guide](./installation.md) for more information.
- Update your GPU drivers from your GPU vendor (Intel/AMD/NVIDIA/Qualcomm).
- Ensure hardware acceleration is enabled in Windows and not blocked by third‑party software.
- Try launching ZZZ with no extensions or custom settings to isolate conflicts.

### Terminal issues

If activation scripts don’t run, update to the latest version and verify your shell profile files are not exiting early. For Git operations, confirm Git Bash or PowerShell is available and on PATH.

### SSH remoting problems

When prompted for credentials, use the graphical askpass dialog. If it doesn’t appear, check for credential manager conflicts and that GUI prompts aren’t blocked by your terminal.

### Graphics issues

#### ZZZ fails to open / degraded performance

ZZZ requires a DirectX 11 compatible GPU to run. If ZZZ fails to open, your GPU may not meet the minimum requirements.

To check if your GPU supports DirectX 11, run the following command:

```
dxdiag
```

This will open the DirectX Diagnostic Tool, which shows the DirectX version your GPU supports under `System` → `System Information` → `DirectX Version`.

If you're running ZZZ inside a virtual machine, it will use the emulated adapter provided by your VM. While ZZZ will work in this environment, performance may be degraded.
