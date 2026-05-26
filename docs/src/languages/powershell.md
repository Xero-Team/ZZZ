---
title: PowerShell
description: "Configure PowerShell language support in Zed, including language servers, formatting, and debugging."
---

# PowerShell

PowerShell language support in Zed is built in and uses [PowerShell Editor Services](https://github.com/PowerShell/PowerShellEditorServices).

- Tree-sitter: [airbus-cert/tree-sitter-powershell](https://github.com/airbus-cert/tree-sitter-powershell)
- Language Server: [PowerShell/PowerShellEditorServices](https://github.com/PowerShell/PowerShellEditorServices)

## Setup

### Install PowerShell 7+ {#powershell-install}

- macOS: `brew install powershell/tap/powershell`
- Alpine: [Installing PowerShell on Alpine Linux](https://learn.microsoft.com/en-us/powershell/scripting/install/install-alpine)
- Debian: [Install PowerShell on Debian Linux](https://learn.microsoft.com/en-us/powershell/scripting/install/install-debian)
- RedHat: [Install PowerShell on RHEL](https://learn.microsoft.com/en-us/powershell/scripting/install/install-rhel)
- Ubuntu: [Install PowerShell on RHEL](https://learn.microsoft.com/en-us/powershell/scripting/install/install-ubuntu)
- Windows: [Install PowerShell on Windows](https://learn.microsoft.com/en-us/powershell/scripting/install/installing-powershell-on-windows)

Zed uses the `pwsh` executable found in your path. PowerShell Editor Services is a PowerShell 7+ module, so this setup works cross-platform anywhere `pwsh` is available.

### PowerShell Editor Services {#powershell-editor-services}

Zed downloads [PowerShell Editor Services](https://github.com/PowerShell/PowerShellEditorServices) automatically into its language-server cache.

You can still configure `powershell-es` through the normal `lsp` settings, including `settings` and `initialization_options` overrides.
