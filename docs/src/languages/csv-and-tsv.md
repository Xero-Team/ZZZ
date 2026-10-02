---
title: Delimited text
description: "Edit, highlight, and preview common delimited text files in ZZZ."
---

# Delimited text

ZZZ includes shared syntax highlighting for comma-separated (`.csv`),
tab-separated (`.tsv`), pipe-separated (`.psv`), and semicolon-separated
(`.scsv` and `.ssv`) files. The grammars highlight headers, values, numbers,
booleans, null-like values, delimiters, quotes, and escaped quotes.

The `.ssv` suffix means semicolon-separated values here. Space-separated text
does not have a reliable quoting convention and is not treated as a table file
automatically.

## Preview tabular data {#preview-tabular-data}

Open a delimited text file and select the eye button in the quick action bar to
open an interactive table preview. Hold `Alt` while selecting the button to
open the preview in a split.

The preview chooses a delimiter from the file suffix for `.tsv`, `.psv`, `.scsv`,
and `.ssv`. For `.csv`, it inspects the first records and selects the stable
delimiter among comma, tab, pipe, and semicolon. A first line such as `sep=;`
has priority and is hidden from the table.

Use the settings bar to override the delimiter, enter a custom single-character
delimiter, or switch between using the first record as headers and displaying
synthetic `Column 1`, `Column 2`, and later names. The preview updates when you
edit the source file and supports quoted fields, escaped quotes, and multiline
records.
