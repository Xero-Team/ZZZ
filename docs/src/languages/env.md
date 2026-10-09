---
title: Env
description: Configure Env language support, including file type detection and highlighting.
---

# Env

ZZZ supports Env files natively.

- Tree-sitter: [tree-sitter/tree-sitter-bash](https://github.com/tree-sitter/tree-sitter-bash)

ZZZ recognizes `.env` and `.env.*` files as Env. Files named `.envrc` are [Shell Script](./sh.md), because [`direnv`](https://direnv.net) executes them as shell.

## Configuration

Env files are highlighted but no language server is started for them. To run shell tooling on them, map them to Shell Script in your settings.json:

```json [settings]
{
  "file_types": {
    "Shell Script": [".env", ".env.*"]
  }
}
```

To recognize other file names as Env files:

```json [settings]
{
  "file_types": {
    "Env": ["*.dotenv"]
  }
}
```

## See also

- [ZZZ Docs: Language Support: Shell Script](./sh.md)
