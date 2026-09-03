---
title: Install ZZZ - macOS, Linux, Windows
description: Build and install ZZZ on macOS, Linux, or Windows from source.
---

# Installing ZZZ

ZZZ does not provide pre-built binaries. Build from this repository.

```sh
cargo run
```

See the local build guides for system dependencies:

- [macOS](./development/macos.md)
- [Linux](./development/linux.md)
- [Windows](./development/windows.md)
- [FreeBSD](./development/freebsd.md)

Automatic updates are disabled by default. There is no hosted download
script, Homebrew cask, or winget package for ZZZ. After you build, you
can run the resulting binary from the source tree.

If you previously installed a binary with `zzz --uninstall` or a package
manager, see [Uninstall](./uninstall.md). For graphics and desktop-portal
issues on Linux, see [Linux](./linux.md).

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

#### Mac Hardware

ZZZ supports machines with Intel (x86_64) or Apple (aarch64) processors
that meet the above macOS requirements:

- MacBook Pro (Early 2015 and newer)
- MacBook Air (Early 2015 and newer)
- MacBook (Early 2016 and newer)
- Mac Mini (Late 2014 and newer)
- Mac Pro (Late 2013 or newer)
- iMac (Late 2015 and newer)
- iMac Pro (all models)
- Mac Studio (all models)

### Linux

ZZZ supports 64-bit Intel/AMD (x86_64) and 64-bit Arm (aarch64)
processors.

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

ZZZ supports machines with x64 (Intel, AMD) or Arm64 (Qualcomm)
processors that meet the following requirements:

- Graphics: A GPU that supports DirectX 11 (most PCs from 2012+).
- Driver: Current NVIDIA/AMD/Intel/Qualcomm driver (not the Microsoft
  Basic Display Adapter).

### FreeBSD

Build [from source](./development/freebsd.md).

### Web

Not supported at this time.
