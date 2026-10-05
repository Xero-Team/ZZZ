---
title: Upstream Sync 2026-10-04
description: Review of the first 20 Zed commits after the September 30 baseline.
---

# Upstream Sync 2026-10-04

## Scope

- Target branch: `sync/upstream-2026-10-04` from local `main` at
  `ae905386d63544b2a0faf70f5d68e9d9def61f94`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `decbf641b18f1982b3475c037e7c5c554471574f`
- Reviewed head: `f8c2cc844057540ca1eac7de4f19f50d7597dead`
- Live upstream head queried: `a84689073d296dfd39987bc7dd478e43ef76d83a`
- Query time: `2026-10-04T17:59:40Z`
- Reviewed range: the first 20 commits after `decbf641b1`, oldest first
- Remaining after this batch: 47 commits through the queried live head

## Decisions

| Upstream   | Class | Local commit           | Disposition                                                                                                                                                                            |
| ---------- | ----- | ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| afecd6d719 | B     | 2be2b9c29a             | Exposed standalone system Node discovery and retained all validation coverage; adapted the lockfile to ZZZ's resolved `thiserror` 2.0.21.                                              |
| 414428c592 | C     | --                     | Requires the absent `ThreadMetadata.title_override` persistence architecture; ZZZ currently makes titles read-only when an ACP agent cannot set them.                                  |
| 5d80b4e784 | B     | 63375b7f5b             | Ported table-cell image alignment and vertical centering into ZZZ's post-Mermaid `push_markdown_image` path.                                                                           |
| 39b5329322 | C     | --                     | Cross-layer ACP v2 submission-receipt and activity rewrite with no current ZZZ transport caller; it also touches native-agent and absent `eval_cli` consumers.                         |
| 749a1df41d | C     | --                     | Delta promotional announcement, external links, telemetry events, and the absent `auto_update_ui` surface.                                                                             |
| f2cdf3d5bd | C     | --                     | V2 authoritative terminal display machinery depends on rejected `39b5329322` and adds terminal replacement APIs with no current ZZZ caller.                                            |
| 15dc0e9fa0 | C     | --                     | The upstream `open_ai` and `language_models` provider crates are absent; ZZZ does not restore this commercial provider route.                                                          |
| e8c6c67fad | C     | --                     | ACP v2 terminal ingress depends on rejected `39b5329322` and `f2cdf3d5bd`; the new v2 builder is deliberately not selected by an application connection.                               |
| 4c0ead4a3e | C     | --                     | Test-only change in the absent GPUI hang-profiler and hang-telemetry architecture.                                                                                                     |
| 9874229eb7 | B     | 2332d915ad             | Ported inlay hint bias selection, colocated-hint normalization, cross-server deduplication, and resolve-position preservation while retaining ZZZ's concurrent chunk invalidation fix. |
| e91af043a2 | A     | d767e80d1b             | Already equivalent: ZZZ's package and lockfile are already at version 1.24.0 through the existing local port.                                                                          |
| 8da42ddee3 | B     | 99052294d2             | Ported offline installed-extension discovery and retry behavior into ZZZ's localized card implementation without restoring telemetry.                                                  |
| d284cbbc87 | C     | --                     | Test-only change in the absent GPUI hang-profiler and hang-telemetry architecture.                                                                                                     |
| 8bbcd83968 | C     | --                     | Unisolatable file-content/search rewrite: it conflicts across 10 files with ZZZ's worktree-owned decoding, custom encoding policy, and `LineHint` search API.                          |
| 66432e4ca9 | C     | --                     | Requires the absent `LocalSnapshot.external_canonical_to_relative` index; ZZZ only has scanner-internal multi-target symlink mappings that cannot safely serve synchronous LSP lookup. |
| 0bdc70c2eb | B     | f08a3944e1             | Ported the current-file hover-link filter and both command-click regressions onto ZZZ's current hover-link layout.                                                                     |
| 8e7fbcc131 | B     | 4d4185cf5f, 44abc705b3 | Retained the EOF language-scope fix and added ZZZ's missing test-only `tree-sitter-c` dependency so the upstream regression test builds.                                               |
| bb7751c379 | C     | --                     | The LM Studio provider crate is absent locally, so the one-line compatibility variant has no ZZZ caller.                                                                               |
| 40180d9c40 | C     | --                     | Upcoming iOS backend infrastructure depends on the absent `gpui_apple` crate and broad Apple renderer/Cargo reshaping.                                                                 |
| f8c2cc8440 | B     | a2a02e001d             | Queued selections while ACP threads load, delivered them after loading or retry, and protected pending-selection drafts from replacement and cleanup.                                  |

Totals: one `A`, seven `B`, twelve `C`.

## Applied work

### `afecd6d719` system Node discovery

`2be2b9c29a` exposes typed standalone discovery of a Node.js 22+ executable,
preserves configured-path and search-path precedence, and reuses the validation
inside the existing system runtime. The upstream lockfile named
`thiserror 2.0.17`; ZZZ already resolves `thiserror 2.0.21`, so the local lockfile
entry follows the actual workspace resolution.

### `5d80b4e784` Markdown table images

`63375b7f5b` tracks whether Markdown rendering is inside a table cell. Images
continue to follow the column's horizontal alignment, become vertically centered
inside tall rows, and retain top alignment outside tables. ZZZ applies the
invariant in `push_markdown_image`, the image path introduced by its newer
Markdown and Mermaid tree.

### `9874229eb7` inlay hint bias

`2332d915ad` selects prefix or suffix bias for type, parameter, and untyped LSP
hints; normalizes all hints at the same buffer offset; re-biases already displayed
hints when a later server introduces a conflict; and deduplicates identical hints
by offset and text rather than anchor identity. The port keeps ZZZ's existing fix
for concurrent invalidating and scrolling hint fetches.

### `8da42ddee3` offline extensions

`99052294d2` computes installed extension results from `ExtensionStore` without
waiting for the registry. The Installed filter therefore works offline, and a
failed All fetch shows local extensions with a localized warning and Retry
action. The upstream ExtensionCard rewrite and telemetry calls remain absent;
ZZZ's existing localized card renderer is retained.

### `0bdc70c2eb` current-file hover links

`f08a3944e1` removes a file hover link only when it resolves to the buffer already
open. Command-click then falls back to references for a symbol named after its
file, while ordinary links to other files continue to navigate.

### `8e7fbcc131` language scope at EOF

`4d4185cf5f` accepts a syntax node's end boundary only when the cursor is at the
end of the buffer, preserving mid-buffer behavior. `44abc705b3` supplies the
test-only C grammar dependency that upstream already had in its language test
graph.

### `f8c2cc8440` selections during ACP startup

`a2a02e001d` stores selected context in `ConversationView` until an active message
editor exists, drains it exactly once after loading or retry, and treats pending
selections as meaningful draft state. The newer upstream thread-removal lifecycle
does not exist locally and was not recreated.

## Rejected work

- `414428c592` cannot persist a user title override because the required metadata
  field and editable unsupported-title path are absent.
- `39b5329322`, `f2cdf3d5bd`, and `e8c6c67fad` form an ACP v2 receipt/activity and
  terminal-notification stack. ZZZ has no v2 transport caller, and importing the
  intermediate public APIs would be unused scaffolding.
- `749a1df41d` is a Delta announcement with promotional, external-link, and
  telemetry behavior.
- `15dc0e9fa0` and `bb7751c379` target provider crates deleted from ZZZ.
- `4c0ead4a3e` and `d284cbbc87` target the deleted hang-profiler/telemetry tests.
- `8bbcd83968` replaces ZZZ's current decoding and search architecture rather than
  supplying an isolatable fix.
- `66432e4ca9` relies on a snapshot symlink index that ZZZ does not have; the local
  scanner's private multi-target map cannot be copied safely into the synchronous
  LSP path lookup.
- `40180d9c40` is infrastructure for an absent future iOS backend.

## Verification

| Check                                                                                                                                           | Result                      |
| ----------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------- |
| Live `git ls-remote` and `git fetch --no-tags` to `FETCH_HEAD`                                                                                  | PASS                        |
| Previous baseline is an ancestor of `FETCH_HEAD`                                                                                                | PASS                        |
| `git diff --check`                                                                                                                              | PASS                        |
| `cargo fmt --all -- --check`                                                                                                                    | PASS                        |
| `cargo check --locked` for `node_runtime`, `markdown`, `editor`, `project`, `extension_host`, `extensions_ui`, `language`, and `agent_ui` tests | PASS                        |
| `cargo test --locked -p node_runtime`                                                                                                           | PASS, 27 tests              |
| Markdown table-state and image-layout regressions                                                                                               | PASS, 3 tests               |
| Inlay normalization and duplicate-cache regressions                                                                                             | PASS, 2 tests               |
| Current-file and other-file command-click regressions                                                                                           | PASS, 2 tests               |
| `cargo test --locked -p extensions_ui --lib`                                                                                                    | PASS, 3 tests               |
| ACP pending-selection retry regression                                                                                                          | PASS, 1 test                |
| `cargo test --locked -p language --lib test_language_scope_at_end_of_buffer`                                                                    | PASS, 1 test                |
| English / Simplified Chinese recursive locale key-set comparison                                                                                | PASS, 3543 scalar keys each |
| `./script/check-philosophy`                                                                                                                     | PASS                        |
| macOS / Windows runtime checks                                                                                                                  | NOT RUN                     |
| `cargo test --workspace`                                                                                                                        | NOT RUN                     |

The general reviewed baseline advances to
`f8c2cc844057540ca1eac7de4f19f50d7597dead`.
