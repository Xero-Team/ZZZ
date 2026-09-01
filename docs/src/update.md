---
title: Update ZZZ
description: "ZZZ does not auto-update. Rebuild from source when you want a newer revision."
---

# Update ZZZ

ZZZ does not ship a hosted updater. Auto-update is off by default, and there
is no download channel that installs a newer binary for you.

## Auto-updates

`auto_update` defaults to `false`. ZZZ does not check a remote host for
updates or install them in the background.

If you enable `auto_update` in settings, there is still no hosted ZZZ
release service. Prefer rebuilding from the source tree you already have.

## How to check your current version

To check which version of ZZZ you are using:

1. Open the Command Palette ({#kb command_palette::Toggle}).
2. Type and select `zzz: about`. A modal appears with your version
   information.

## How to get a newer build

Build from this repository:

```sh
cargo run
```

See the local build guides for system dependencies:

- [macOS](./development/macos.md)
- [Linux](./development/linux.md)
- [Windows](./development/windows.md)

To confirm auto-update stays off, open the Settings Editor
({#kb zed::OpenSettings}) and search for `Auto Update` under General
Settings.

Or add this to your settings.json:

```json [settings]
{
  "auto_update": false
}
```
