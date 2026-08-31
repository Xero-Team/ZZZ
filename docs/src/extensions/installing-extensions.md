---
title: Installing Extensions
description: "Browse, install, and manage extensions from the ZZZ Extension Gallery."
---

# Installing Extensions {#installing-extensions}

Extensions add functionality to ZZZ, including languages, themes, and AI tools. Browse and install them from the Extension Gallery.

Open the Extension Gallery with {#kb zed::Extensions}, or select "ZZZ > Extensions" from the menu bar.

The gallery lists and downloads extensions from the public Zed marketplace at `https://api.zed.dev`. ZZZ does not install or update extensions unless you request it.

## Installation Location

- On macOS, extensions are installed in `~/Library/Application Support/ZZZ/extensions`.
- On Linux, they are installed in `~/.local/share/zzz/extensions`.
- On Windows, the directory is `%LOCALAPPDATA%\ZZZ\extensions`.

This directory contains two subdirectories:

- `installed`, which contains the source code for each extension.
- `work` which contains files created by the extension itself, such as downloaded language servers.
