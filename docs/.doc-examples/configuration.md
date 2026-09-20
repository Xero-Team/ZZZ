<!--
  GOLD STANDARD EXAMPLE: Configuration Documentation

  This example demonstrates documentation for settings and configuration.

  Key patterns to note:
  - Anchor IDs on all major sections
  - Opening paragraph explains what this guide covers
  - Multiple JSON examples with [settings] annotation
  - Platform-specific file paths
  - Proper callout formatting
  - "See Also" section at the end (not "What's Next")
-->

---

title: Configuring ZZZ - Settings and Preferences
description: Configure ZZZ with the Settings Editor, JSON files, and project-specific overrides. Covers all settings options.

---

# Configuring ZZZ

This guide explains how ZZZ's settings system works, including the Settings Editor, JSON configuration files, and project-specific settings.

For visual customization (themes, fonts, icons), see [Appearance](./appearance.md).

## Settings Editor {#settings-editor}

The **Settings Editor** ({#kb zzz::OpenSettings}) is the primary way to configure ZZZ. It provides a searchable interface where you can browse available settings, see their current values, and make changes.

To open it:

- Press {#kb zzz::OpenSettings}
- Or run `zzz: open settings` from the command palette

As you type in the search box, matching settings appear with descriptions and controls to modify them. Changes save automatically to your settings file.

> **Note:** Not all settings are available in the Settings Editor yet. Some advanced options, like language formatters, require editing the JSON file directly.

## Settings Files {#settings-files}

### User Settings {#user-settings}

Your user settings apply globally across all projects. Open the file with {#kb zzz::OpenSettingsFile} or run `zzz: open settings file` from the command palette.

The file is located at:

- macOS: `~/.config/zzz/settings.json`
- Linux: `~/.config/zzz/settings.json` (or `$XDG_CONFIG_HOME/zzz/settings.json`)
- Windows: `%APPDATA%\ZZZ\settings.json`

The syntax is JSON with support for `//` comments.

### Default Settings {#default-settings}

To see all available settings with their default values, run {#action zzz::OpenDefaultSettings} from the command palette. This opens a read-only reference you can use when editing your own settings.

### Project Settings {#project-settings}

Override user settings for a specific project by creating a `.zzz/settings.json` file in your project root. Run {#action zzz::OpenProjectSettings} to create this file.

Project settings take precedence over user settings for that project only.

```json [settings]
// .zzz/settings.json
{
  "tab_size": 2,
  "formatter": "prettier",
  "format_on_save": "on"
}
```

You can also add settings files in subdirectories for more granular control.

> **Note:** Not all settings can be set at the project level. Settings that affect the editor globally (like `theme` or `vim_mode`) only work in user settings. Project settings are limited to editor behavior and language tooling options like `tab_size`, `formatter`, and `format_on_save`.

## How Settings Merge {#how-settings-merge}

Settings are applied in layers:

1. **Default settings** — ZZZ's built-in defaults
2. **User settings** — Your global preferences
3. **Project settings** — Project-specific overrides

Later layers override earlier ones. For object settings (like `terminal`), properties merge rather than replace entirely.

## Per-Release Channel Overrides {#release-channel-overrides}

Use different settings for Stable or Dev builds by adding top-level channel keys:

```json [settings]
{
  "theme": "One Dark",
  "vim_mode": false,
  "dev": {
    "theme": "Rosé Pine",
    "vim_mode": true
  }
}
```

With this configuration:

- **Stable** uses One Dark with vim mode off
- **Dev** uses Rosé Pine with vim mode on

Changes made in the Settings Editor apply across all channels.

## Settings Deep Links {#deep-links}

ZZZ supports deep links that open specific settings directly:

```
zzz://settings/theme
zzz://settings/vim_mode
zzz://settings/buffer_font_size
```

These are useful for sharing configuration tips or linking from documentation.

## Example Configuration {#example-configuration}

```json [settings]
{
  "theme": {
    "mode": "system",
    "light": "One Light",
    "dark": "One Dark"
  },
  "buffer_font_family": "JetBrains Mono",
  "buffer_font_size": 14,
  "tab_size": 2,
  "format_on_save": "on",
  "autosave": "on_focus_change",
  "vim_mode": false,
  "terminal": {
    "font_family": "JetBrains Mono",
    "font_size": 14
  },
  "languages": {
    "Python": {
      "tab_size": 4
    }
  }
}
```

## See Also {#see-also}

- [Appearance](./appearance.md) — Themes, fonts, and visual customization
- [Key bindings](./key-bindings.md) — Customize keyboard shortcuts
- [AI Configuration](./ai/configuration.md) — Set up AI providers, models, and agent settings
- [All Settings](./reference/all-settings.md) — Complete settings reference
