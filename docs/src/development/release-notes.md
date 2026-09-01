---
title: Release Notes
description: "Guide to release notes for ZZZ development."
---

# Release Notes

ZZZ does not run hosted Zed preview or stable release automation. There is
no weekly preview-channel collector and no `zed.dev/releases` publication
step for this repository.

When you open a pull request, describe user-visible changes in the pull
request body so reviewers and later readers can see what landed:

```md
...

Release Notes:

- N/A _or_ Added/Fixed/Improved ...
```

Use `N/A` when the change is not user-visible (docs, tests, internal
refactors).

## Guidelines for crafting your `Release Notes` line(s)

- A `Release Notes` line should only be written if the user can see or
  feel the difference in ZZZ.
- A `Release Notes` line should be written such that a ZZZ user can
  understand what the change is. Don't assume a user knows technical
  editor developer lingo; phrase your change in language they understand
  as a user of a text editor.
- If you want to include technical details about your pull request for
  other contributors to see, do so above the `Release Notes` line.
- Changes to docs should be labeled as `N/A`.
- If your pull request adds/changes a setting or a keybinding, always
  mention that setting or keybinding. Don't make the user dig into docs
  or the pull request to find this information (although it should be
  included in docs as well).
- For pull requests that are reverts:
  - If the item being reverted **has already been shipped**, include a
    `Release Notes` line explaining why we reverted, as this is a
    breaking change.
- If the item being reverted **hasn't been shipped**, edit the original
  PR's `Release Notes` line to `N/A`.
