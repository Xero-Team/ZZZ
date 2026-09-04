---
title: Code Completions - ZZZ
description: ZZZ's code completions from language servers and edit predictions. Configure autocomplete behavior, snippets, and documentation display.
---

# Completions

ZZZ supports two sources for completions:

1. "Code Completions" provided by language servers (LSPs) you download
   explicitly, or via [ZZZ Language Extensions](languages.md). Language
   servers are not downloaded when you open a buffer.
2. "Edit Predictions" from a provider you configure. Prefer a local
   endpoint such as Ollama or llama.cpp. GitHub Copilot and Codestral are
   optional and only used after you select them.

## Language Server Code Completions {#code-completions}

When there is an appropriate language server available, ZZZ will provide
completions of variable names, functions, and other symbols in the current
file. You can disable these by adding the following to your ZZZ
`settings.json` file:

```json [settings]
"show_completions_on_input": false
```

You can manually trigger completions with `ctrl-space` or by triggering the
`editor::ShowCompletions` action from the command palette.

> Note: Using `ctrl-space` in ZZZ requires disabling the macOS global
> shortcut. Open **System Settings** > **Keyboard** > **Keyboard
> Shortcut**s > **Input Sources** and uncheck **Select the previous input
> source**.

For more information, see:

- [Configuring Supported Languages](./configuring-languages.md)
- [List of ZZZ Supported Languages](./languages.md)

## Edit Predictions {#edit-predictions}

Edit predictions appear as you type once you configure a provider. Most of
the time, you can accept them by pressing `tab`.

See the [edit predictions documentation](./ai/edit-prediction.md) for how
to set up a local provider and optional remote providers.
