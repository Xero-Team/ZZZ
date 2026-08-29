---
title: Install ZZZ - macOS, Linux, Windows
description: Build and install ZZZ on macOS, Linux, or Windows using local instructions.
---

# Installing ZZZ

## Download ZZZ

### macOS

Build and install ZZZ locally from this repository. Automatic updates are disabled by default.

If a local package is available for your platform, install the ZZZ package:

```sh
brew install --cask zzz
```

### Windows

Use the platform-specific build instructions below; no hosted download or update check is required.

Additionally, you can install ZZZ using winget:

```sh
winget install -e --id ZZZEditor.ZZZ
```

### Linux

For most Linux users, the easiest way to install ZZZ is through our installation script:

```sh
./script/install.sh
```

You can now optionally specify a **version** of ZZZ to install using the `ZED_VERSION` environment variable:

```sh
# Install the latest stable version (default)
./script/install.sh

# Install a specific version from a locally checked-out source tree
ZED_VERSION=0.216.0 ./script/install.sh
```

This script supports `x86_64` and `AArch64`, as well as common Linux distributions: Ubuntu, Arch, Debian, RedHat, CentOS, Fedora, and more.

If ZZZ is installed using this installation script, it can be uninstalled at any time by running the shell command `zzz --uninstall`. The shell will then prompt you whether you'd like to keep your preferences or delete them. After making a choice, you should see a message that ZZZ was successfully uninstalled.

If this script is insufficient for your use case, you run into problems running ZZZ, or there are errors in uninstalling ZZZ, please see our [Linux-specific documentation](./linux.md).

## System Requirements

### macOS

ZZZ supports the follow macOS releases:

| Version       | Codename | Apple Status   | ZZZ Status          |
| ------------- | -------- | -------------- | ------------------- |
| macOS 26.x    | Tahoe    | Supported      | Supported           |
| macOS 15.x    | Sequoia  | Supported      | Supported           |
| macOS 14.x    | Sonoma   | Supported      | Supported           |
| macOS 13.x    | Ventura  | Supported      | Supported           |
| macOS 12.x    | Monterey | EOL 2024-09-16 | Supported           |
| macOS 11.x    | Big Sur  | EOL 2023-09-26 | Partially Supported |
| macOS 10.15.x | Catalina | EOL 2022-09-12 | Partially Supported |

The macOS releases labelled "Partially Supported" (Big Sur and Catalina) do not support screen capture features that rely on the [LiveKit SDK](https://livekit.io). That SDK depends on [ScreenCaptureKit.framework](https://developer.apple.com/documentation/screencapturekit/), which is only available on macOS 12 (Monterey) and newer.

#### Mac Hardware

ZZZ supports machines with Intel (x86_64) or Apple (aarch64) processors that meet the above macOS requirements:

- MacBook Pro (Early 2015 and newer)
- MacBook Air (Early 2015 and newer)
- MacBook (Early 2016 and newer)
- Mac Mini (Late 2014 and newer)
- Mac Pro (Late 2013 or newer)
- iMac (Late 2015 and newer)
- iMac Pro (all models)
- Mac Studio (all models)

### Linux

ZZZ supports 64-bit Intel/AMD (x86_64) and 64-bit Arm (aarch64) processors.

ZZZ requires a Vulkan 1.3 driver and the following desktop portals:

- `org.freedesktop.portal.FileChooser`
- `org.freedesktop.portal.OpenURI`
- `org.freedesktop.portal.Secret` or `org.freedesktop.Secrets`

### Windows

ZZZ supports the following Windows releases:

| Version                            | ZZZ Status |
| ---------------------------------- | ---------- |
| Windows 11, version 22H2 and later | Supported  |
| Windows 10, version 1903 and later | Supported  |

A 64-bit operating system is required to run ZZZ.

#### Windows Hardware

ZZZ supports machines with x64 (Intel, AMD) or Arm64 (Qualcomm) processors that meet the following requirements:

- Graphics: A GPU that supports DirectX 11 (most PCs from 2012+).
- Driver: Current NVIDIA/AMD/Intel/Qualcomm driver (not the Microsoft Basic Display Adapter).

### FreeBSD

Not yet available as an official download. Can be built [from source](./development/freebsd.md).

### Web

Not supported at this time. See our [Platform Support issue](https://github.com/zed-industries/zed/issues/5391).
