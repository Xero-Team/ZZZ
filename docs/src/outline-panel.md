---
title: Outline Panel - ZZZ
description: Navigate code structure with ZZZ's outline panel. View symbols, jump to definitions, and browse file outlines.
---

# Outline Panel

In addition to the modal outline (`cmd-shift-o`), ZZZ offers an outline panel. The outline panel can be deployed via `cmd-shift-b` (`outline panel: toggle focus` via the command palette), or by clicking the `Outline Panel` button in the status bar.

When viewing a "singleton" buffer (i.e., a single file on a tab), the outline panel works similarly to that of the outline modal－it displays the outline of the current buffer's symbols. Each symbol entry shows its type prefix (such as "struct", "fn", "mod", "impl") along with the symbol name, helping you quickly identify what kind of symbol you're looking at. Clicking on an entry allows you to jump to the associated section in the file. The outline view will also automatically scroll to the section associated with the current cursor position within the file.

## Usage with multibuffers

The outline panel truly excels when used with multi-buffers. Here are some examples of its versatility:

### Project Search Results

Get an overview of search results across your project.

### Project Diagnostics

View a summary of all errors and warnings reported by the language server.

### Find All References

Quickly navigate through all references when using the `editor: find all references` action.

The outline view provides a great way to quickly navigate to specific parts of your code and helps you maintain context when working with large result sets in multi-buffers.
