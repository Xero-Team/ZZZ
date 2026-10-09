---
title: Typst
description: "Typst language support in ZZZ, including syntax highlighting, the Tinymist language server, and document preview."
---

# Typst

Typst support is built into ZZZ.

Built-in syntax support includes highlighting, outlines, indentation, raw block
language injection, bracket matching, and syntax-aware text objects. Tinymist
provides language server features.

Typst is a typesetting system that is similar to LaTeX in spirit, but tends to
be easier to get started with. For more information see
[typst.app](https://typst.app/).

- Tree-sitter: [SeniorMars/tree-sitter-typst](https://github.com/SeniorMars/tree-sitter-typst)
- Language Server: [tinymist](https://myriad-dreamin.github.io/tinymist/)

## Preview {#preview}

ZZZ compiles Typst documents with its built-in Typst compiler and shows all
pages in a read-only preview. The preview works independently of Tinymist and
supports local and remote projects.

Open the command palette and run {#action typst::OpenPreview} or
{#action typst::OpenPreviewToTheSide}. You can also use the preview button in
the editor toolbar or **Open Typst Preview** in the editor and Project Panel
context menus.

Run {#action typst::OpenFollowingPreview} to keep one preview attached to the
most recently focused Typst editor.

Save the main file once before opening its preview. Afterwards, edits to open
Typst files in the same project refresh the preview after a short delay without
writing to disk. Other imports and assets are resolved from their saved
contents in the worktree that contains the main file.

The preview surface, default page and text colors, and status messages follow
the active ZZZ theme. Colors explicitly set by the Typst document override
these preview defaults.

Typst packages already present in the standard package directories or cache
are loaded automatically. When a package is missing, ZZZ displays a download
action. The package is downloaded only after you select that action.
