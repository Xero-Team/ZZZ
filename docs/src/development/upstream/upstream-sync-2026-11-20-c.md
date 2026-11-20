---
title: Upstream Sync 2026-11-20 (C)
description: Selective Zed upstream sync, the three commits after the 2026-11-20 batch B.
---

# Upstream Sync 2026-11-20 (C)

## Scope

- Local base: `sync/upstream-2026-11-20` at `00527d384c`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `0c5af8c5634df7069a7b51794920e80128bf1081`
- Reviewed range: `0c5af8c563..4a7d10473a`, 3 commits, oldest first
- New reviewed baseline: `4a7d10473a3ed32c10026e549381f064dd020114`
- Remaining after this batch: 24 commits, starting with `df1ba676f2`

This batch is short by design. It opens with the language/markdown/text
dead-code sweep that ended the previous batch, which needed a pass of its
own because it conflicts with deliberate local rewrites. Two of its three
commits then turn out to target crates the fork does not carry.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                                                                   |
| ---------- | ----- | ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 36acea54c5 | B     | a603d0bca6   | Removes dead code across the language, markdown, and text crates. Payload below.                                                                                                                                                              |
| 886dab4069 | C     | --           | Removes dead code from the edit-prediction crates. Local ships no `edit_prediction`, `edit_prediction_context`, `edit_prediction_ui`, `edit_prediction_cli`, `edit_prediction_metrics`, or `zeta_prompt`, which the ACP-only cleanup deleted. |
| 4a7d10473a | C     | --           | Removes the sandbox prompt timeout. Local has no `crates/sandbox_linux`.                                                                                                                                                                      |

Totals: zero `A`, one `B`, two `C`.

## Applied Work

- `36acea54c5` is a `B` port and carries `git cherry-pick -s` sign-off
  for DCO.

No remote, pull request, or named upstream remote was created.

## Per-Commit Narrative

### 36acea54c5 — dead code in language and text crates (B)

Retained: drops the unused `HighlightedText::from_buffer_range` and its
`Buffer::highlighted_text_for_range` caller, `Buffer::build_empty_snapshot`
and `Buffer::build_snapshot_sync`, `RowChunks::previous_chunk`, the
`disconnected_peers` bookkeeping on the text network simulator, and the
`LANGUAGE_GIT_COMMIT` grammar, together with the redundant `to_owned`
calls and the empty `test-support` feature sections they left behind. The
`languages` crate no longer depends on `tree-sitter-gitcommit` or offers
it in `load-grammars`; `crates/grammars` keeps the grammar for the
languages that still register it.

Omitted: `cached_highlight_runs`, `compute_chunk_highlights`, and
`ResolvedHighlights`, which sit beside the removed helper in the upstream
pre-image but were never absorbed locally and reference `HighlightRun`,
`HighlightId`, and `MAX_BYTES_TO_HIGHLIGHT_IN_A_CHUNK`, none of which this
tree defines. Local's `RowChunks` is an eager `Arc<[RowChunk]>` rather
than the lazy computed variant, so only `previous_chunk` is dropped there
and `chunk_row_range`/`chunk` are not introduced. `toolchain_selector`
keeps `use i18n::tr;` because local localizes the labels.

Two conflicts needed care rather than a side pick. `markdown_preview`
keeps its real `test-support = ["db/test-support"]` feature, since only
the duplicate empty section is dead, and `language_core` keeps `util` and
`vfs` as dependencies while dropping the empty feature. The `text`
network simulator keeps `use rand::RngExt;`, which supplies the
`random_range` calls the removal does not touch.

## Verification

| Check                                                                                                                                                                                     | Result  | Notes                             |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------- | --------------------------------- |
| `cargo check -p language -p languages -p text -p markdown -p markdown_preview -p toolchain_selector -p html_to_markdown -p buffer_diff -p rope -p sum_tree -p fuzzy_nucleo --all-targets` | PASS    | clean after the port              |
| `cargo test -p text --lib`                                                                                                                                                                | PASS    | 38 passed                         |
| `cargo test -p markdown --lib`                                                                                                                                                            | PASS    | 225 passed                        |
| `cargo fmt --all -- --check`                                                                                                                                                              | PASS    |                                   |
| `cargo check` for the edit-prediction and sandbox crates                                                                                                                                  | NOT RUN | crates absent from the local tree |
