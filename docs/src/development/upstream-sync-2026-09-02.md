---
title: Upstream Sync 2026-09-02
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-09-02

## Scope

- Target branch: `sync/upstream-2026-09-02` from `main` at
  `7bc9ed5d1df59ad76b4fdc3fef22db2a127a9f45`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previously reviewed baseline: `669bede5c582140d2f3917e2c5fae5b764f9023f`
- Reviewed upstream head: `bce0c5785bfd9172c939aca4083fd70bc4930927`
- Live upstream head queried: `23aca9895c8e9001845282e3d9f13d5c99641b58`
- Query time: `2026-09-02T22:50:24+02:00`
- Reviewed range: `669bede5..bce0c5785`

This batch covered the first 20 commits after the previous baseline. Counts:
4 A, 7 B, and 9 C. The reviewed baseline is now
`bce0c5785bfd9172c939aca4083fd70bc4930927`; 55 commits remain through the
queried live head.

## Decisions

| Upstream | Class | Local commit       | Disposition                                                                                                   |
| -------- | ----- | ------------------ | ------------------------------------------------------------------------------------------------------------- |
| 8cfa848c | C     | --                 | Broad provider-schema rewrite spans cloud/Copilot paths and ZZZ's divergent language-model APIs.              |
| 9e9b13c5 | C     | --                 | Requires upstream Markdown code-chip machinery absent from ZZZ.                                               |
| 956a49e4 | C     | --                 | Touch/Web IME and gesture architecture conflicts with ZZZ's existing web input path.                          |
| a3f6b96c | C     | --                 | Optimization targets deleted `editor/src/input.rs`, absent `git_ui_core`, and a divergent toolchain selector. |
| 49469000 | C     | --                 | Large multibuffer anchor/display rewrite is not isolatable from ZZZ's local layout.                           |
| a61e2609 | B     | 81f19e7b           | Normalize reversed LSP ranges on existing editor, project, document-color, and Copilot paths.                 |
| aa347c30 | B     | 0902ba6d, b96e2cc0 | Preserve single-file breakpoint paths; adapt to local `RelPathBuf`.                                           |
| ca2b5741 | C     | --                 | New upstream lint/test infrastructure only; no independent product behavior.                                  |
| 2890c340 | C     | --                 | Upstream `.rules` instruction is repository scaffolding, not ZZZ product behavior.                            |
| e3adf43f | A     | 885d0870           | Cherry-picked with `-x -s`.                                                                                   |
| 1662f5f3 | B     | 6cb671af, 5d0cf3de | Accept full SHA-256 commit searches and add the local hash-length constant.                                   |
| 31a32671 | A     | 0518256b           | Cherry-picked with `-x -s`.                                                                                   |
| 82aeef2d | A     | 58305b29           | Cherry-picked with `-x -s`.                                                                                   |
| 9dbc9e76 | A     | --                 | Already equivalent: the migration is already absent and `m_2025_06_27` already uses command shape detection.  |
| 19e458b7 | B     | 38fb5544           | Include selected scopes in MCP OAuth dynamic client registration.                                             |
| 399258fe | B     | 599bb8bd           | Reduce closure-funnel monomorphization in existing editor/settings paths.                                     |
| 68b65045 | C     | --                 | Bundle scripts have diverged substantially; Rust bootstrap/config plumbing is not isolatable.                 |
| 770aac34 | C     | --                 | Hosted `cloud_api_client` websocket change is outside ZZZ's local-first boundary.                             |
| 9a76a39d | B     | ad2a3a2f           | Underline diagnostics that span only a line terminator; adapt `Unclipped<PointUtf16>`.                        |
| bce0c578 | B     | f4cd1df5           | Recognize `.bash_login` as a Shell Script filename in a local sync commit.                                    |

## Applied work

Direct A commits `e3adf43f`, `31a32671`, and `82aeef2d` were absorbed with
`git cherry-pick -x -s`. The
`9dbc9e76` behavior was already present locally, so no code change was made.

B ports retain `Upstream`, `Retained`, and `Omitted` trailers:

- `a61e2609`: normalized server ranges before clipping anchors. The deleted
  upstream navigation module and its end-to-end harness test were omitted.
- `aa347c30`: retained the single-file/multiple-worktree path behavior and
  adapted the local relative-path type.
- `1662f5f3`: retained SHA-256 query support and supplied ZZZ's missing
  `SHA256_HEX_LENGTH` constant.
- `19e458b7`: retained MCP OAuth scope propagation and adapted local DCR tests.
- `399258fe`: retained closure-funnel reductions while omitting the divergent
  `EnumVariantDropdown` rewrite.
- `9a76a39d`: retained line-terminator diagnostic highlighting and adapted
  point wrapper types; the regression test uses ZZZ's struct layout.
- `bce0c578`: retained the shell-script filename mapping in a local sync
  commit.

## Verification

```text
PASS git merge-base --is-ancestor 669bede5 FETCH_HEAD
PASS cargo check --locked -p command_palette -p git -p theme -p fs -p context_server -p project -p debugger_ui -p editor -p copilot
PASS cargo test --locked -p command_palette test_commands_sorted_by_recency
PASS cargo test --locked -p git test_commit_hash_search_query_accepts_sha1_and_sha256_hashes
PASS cargo test --locked -p fs test_fake_fs_rename
PASS cargo test --locked -p context_server test_dcr_registration_body_mirrors_server_scopes
PASS cargo test --locked -p project test_diagnostic_range_spanning_line_terminator
PASS git diff --check
NOT RUN macOS / Windows / wasm32 gpui_web runtime
NOT RUN cargo test --workspace
NOT RUN cargo fmt --check (known edition-2024 formatting drift on this host)
```

The reviewed baseline is `bce0c5785bfd9172c939aca4083fd70bc4930927`.
Work remains on `sync/upstream-2026-09-02` and has not been merged to `main`.

## Continuation Run

- Target branch: `sync/upstream-2026-09-02` (continued from the prior run)
- Previously reviewed baseline: `bce0c5785bfd9172c939aca4083fd70bc4930927`
- Reviewed upstream head: `5e28272c1407ced4bae4a90deaea25352a1fbc96`
- Live upstream head queried: `b3326e13c142fc8f313aca67a93dd6855a1e7e32`
- Query time: `2026-09-02T23:11:35+02:00`
- Reviewed range: `bce0c578..5e28272c`

This continuation reviewed the next 20 commits. Counts: 1 A, 5 B, and
14 C. The reviewed baseline is now `5e28272c1407ced4bae4a90deaea25352a1fbc96`;
36 commits remain through the queried live head.

### Decisions

| Upstream | Class | Local commit                           | Disposition                                                                                                                                              |
| -------- | ----- | -------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 6840b8d2 | C     | --                                     | Repository Danger CI and mixed upstream documentation; the documentation also retains hosted-service and telemetry guidance outside ZZZ's boundary.      |
| ded896cc | C     | --                                     | Depends on an unabsorbed incremental-search architecture and divergent editor APIs; not isolatable onto the current search implementation.               |
| e6690f38 | C     | --                                     | Targets the deleted `tabular_data_preview` crate and cannot land without restoring an absent product surface.                                            |
| 24254879 | B     | 2078d56f                               | Filter editorconfig resolution to the keys ZZZ consumes and return `None` when no config exists; omit the divergent buffer cache and benchmark plumbing. |
| 7aa903ef | C     | --                                     | Adds keybindings for the deleted `tabular_data_preview` product surface.                                                                                 |
| adc72a5a | C     | --                                     | CSV preview support requires the absent tabular preview crate and related project-less architecture.                                                     |
| ef075910 | C     | --                                     | Web font ownership rewrite is coupled to upstream examples and has no independent ZZZ web caller; local examples are intentionally removed.              |
| 98c6c140 | B     | c209e22d, 5ff019a2                     | Gate synchronous blocking APIs on wasm and run quit handlers asynchronously; adapt to ZZZ's scheduler naming.                                            |
| a60addb9 | C     | --                                     | objc2 prompt migration conflicts with ZZZ's macOS platform layout and dependency feature set; no safe isolated port was established on this Linux host.  |
| e8fbacb6 | C     | --                                     | Large outline-panel rewrite is not isolatable from ZZZ's divergent panel implementation.                                                                 |
| ff020dd0 | B     | 794b92de                               | Batch ordered conflict-anchor conversion and use it for conflict highlighting; omit upstream benchmark-generator tooling and duplicate tests.            |
| 81df6f4a | C     | --                                     | Replaces crash user data with Sentry tags, which is telemetry machinery rejected by ZZZ.                                                                 |
| 3f00b5d7 | B     | 7740ff10, d6df7d2e, b8f5825a, 81ffe4ce | Restrict row-highlight expansion to the viewport and skip header rows; adapt stored-color and block visibility APIs.                                     |
| 76b1096c | C     | --                                     | Requires an unabsorbed gesture/prediction architecture and divergent GPUI web input APIs.                                                                |
| ee6badf4 | C     | --                                     | Corgi build support is upstream build/release infrastructure with no ZZZ product behavior.                                                               |
| 9785475c | C     | --                                     | Terminal Threads title editing belongs to the rejected native-agent/thread surface.                                                                      |
| ce48461e | A     | 30ed1fca                               | Cherry-picked with `-x -s`; malformed shell-variable references now pass through without panicking.                                                      |
| a66fb6ae | C     | --                                     | Depends on unabsorbed touch/IME gesture state and cannot be isolated from the current web event path.                                                    |
| f8c27835 | C     | --                                     | Large git-panel multi-select rewrite conflicts with ZZZ's divergent panel and is not safely isolatable in this batch.                                    |
| 5e28272c | C     | --                                     | Touch-axis locking depends on the unabsorbed gesture physics rewrite.                                                                                    |

### Applied work

- `24254879`: retained relevant-key editorconfig filtering and the no-config fast path. Omitted the buffer-level `LanguageSettings` cache, benchmark fixture, lockfile/dependency churn, and broad constructor migration.
- `98c6c140`: retained wasm-safe shutdown and API gating for GPUI and scheduler blocking calls; adapted `schedule_foreground` and `ForegroundExecutor` names used by ZZZ.
- `ff020dd0`: retained a single ordered multibuffer excerpt sweep for conflict anchors; omitted the upstream synthetic benchmark script and test-only additions.
- `3f00b5d7`: retained viewport-pruned row highlighting and header-row exclusion; added small visibility adaptations for ZZZ's block-map boundaries.
- `ce48461e`: direct A absorption with `git cherry-pick -x -s`.

### Verification

```text
PASS cargo check --locked -p settings -p scheduler -p gpui -p multi_buffer -p git_ui -p editor -p util
PASS cargo check --locked -p settings
PASS cargo check --locked -p scheduler -p gpui
PASS cargo check --locked -p gpui --target wasm32-unknown-unknown
BLOCKED cargo check --locked -p scheduler --target wasm32-unknown-unknown (the workspace's `getrandom` configuration rejects `wasm32-unknown-unknown` without the `wasm_js` feature)
PASS cargo test --locked -p editor test_highlighted_display_rows_in_range
PASS cargo test --locked -p util test_to_shell_variable_malformed_is_passed_through
PASS cargo test --locked -p git_ui conflict
PASS cargo test --locked -p scheduler
PASS git diff --check
NOT RUN macOS / Windows / wasm32 runtime tests
NOT RUN cargo test --workspace
```

The reviewed baseline is `5e28272c1407ced4bae4a90deaea25352a1fbc96`.
Work remains on `sync/upstream-2026-09-02` and has not been merged to `main`.

## Continuation Run 2

- Target branch: `sync/upstream-2026-09-02`
- Previously reviewed baseline: `5e28272c1407ced4bae4a90deaea25352a1fbc96`
- Reviewed upstream head: `c3cf80c0d1be43f3b84e21ef2c82e91b0d78e788`
- Live upstream head queried: `003826320f320dace212a67e82b0db92cb457081`
- Query time: `2026-09-02T23:45:53+02:00`
- Reviewed range: `5e28272c..c3cf80c`

This continuation reviewed the next 20 commits. Counts: 3 A, 7 B, and
10 C. The reviewed baseline is now `c3cf80c0d1be43f3b84e21ef2c82e91b0d78e788`;
17 commits remain through the queried live head.

### Decisions

| Upstream | Class | Local commit       | Disposition                                                                                                                                              |
| -------- | ----- | ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| c4bc69e5 | C     | --                 | Corgi sandbox/glob and bootstrap configuration is upstream build infrastructure without independent ZZZ product behavior.                                |
| 22945084 | C     | --                 | Broad BufferDiff/GitStore/editor hunk-operation refactor is not isolatable onto ZZZ's divergent diff architecture.                                       |
| a49de953 | B     | f7c61d2e           | Enabled Wayland backend logging while retaining ZZZ's newer dependency versions; the upstream lockfile conflicted.                                       |
| 8ce383f2 | B     | 4bc73f19, 76666dbf | Added and documented `close_panel_on_toggle`, adapting the settings UI to ZZZ's `UiText` and field shape.                                                |
| d1446d66 | B     | c2042a37, cd1d0eee | Queried the configured Ollama server for FIM-capable models and gated the provider on edit-prediction settings; omitted Sweep support absent from ZZZ.   |
| 0855410c | C     | --                 | Long-press gesture recognition is coupled to the unabsorbed touch/IME gesture architecture.                                                              |
| 283460f5 | B     | f4fc7594, cd1d0eee | Added Anthropic Fable 5.1 thinking-binding controls and forced-tool gating; omitted unavailable compaction metadata and deleted Copilot product changes. |
| d3e54c97 | C     | --                 | Requires the absent `fs_embed!` architecture; ZZZ uses direct rust-embed paths.                                                                          |
| 239d0aa1 | C     | --                 | Release/test workflow changes are upstream CI and SDK bootstrap infrastructure.                                                                          |
| ac5af8b9 | C     | --                 | License-manifest and symlink relicensing has no independent ZZZ product behavior and conflicts with current package metadata.                            |
| 2551721a | C     | --                 | Follow-up `fs_embed!` debug embedding depends on the absent upstream macro surface.                                                                      |
| 520d8bda | A     | 28eab08d           | Cherry-picked with `-x -s`; corrected fractional-scale avatar borders.                                                                                   |
| 97b1e64a | C     | --                 | Adds hang-trigger telemetry and release telemetry plumbing, rejected by ZZZ's no-telemetry boundary.                                                     |
| d56dca80 | C     | --                 | GitHub issue-triage workflow only; no product behavior.                                                                                                  |
| 83726412 | A     | 777a6117           | Cherry-picked with `-x -s`; preserves the correct pre-modal focus handle.                                                                                |
| a24cafa9 | B     | 29611f1d           | Removed the completion-row shrink flag using ZZZ's existing flex API spelling.                                                                           |
| 480d81bf | C     | --                 | ChatGPT Subscription setup/cancellation is an explicitly rejected account-bound surface.                                                                 |
| dbfeae77 | B     | 72e44182           | Avoided persisting commit-template text as a user draft; adapted the logic to ZZZ's git panel.                                                           |
| 6309c722 | B     | 1c05fa31, cd1d0eee | Classified Anthropic prompt-too-long HTTP 400s using ZZZ's existing completion error model.                                                              |
| c3cf80c0 | A     | 0026815a           | Cherry-picked with `-x -s`; Git Graph now participates in pane navigation history.                                                                       |

### Applied work

- Direct A commits `520d8bda`, `83726412`, and `c3cf80c0` were absorbed with
  `git cherry-pick -x -s`.
- B ports retain `Upstream`, `Retained`, and `Omitted` trailers. The Ollama,
  Anthropic, modal-focus, completion-layout, Git-template, and prompt-error
  ports each preserve the local behavior without importing rejected account,
  telemetry, Sweep, or absent `fs_embed!` machinery.
- `cd1d0eee` is a formatting-only cleanup for the local B ports.

### Verification

```text
PASS cargo check --locked -p gpui_linux
PASS cargo check --locked -p edit_prediction -p settings_ui -p edit_prediction_ui -p zzz
PASS cargo check --locked -p anthropic -p language_models -p language_models_cloud
PASS cargo check --locked -p editor
PASS cargo check --locked -p git_ui
PASS cargo check --locked -p language_model_core
PASS cargo test --locked -p language_model_core
PASS cargo test --locked -p git_graph test_go_back_from_commit_view_returns_to_git_graph
PASS git diff --check
NOT RUN macOS / Windows / wasm32 runtime tests
NOT RUN cargo test --workspace
NOT RUN full cargo fmt check (known unrelated formatting drift remains outside this run)
```

The reviewed baseline is `c3cf80c0d1be43f3b84e21ef2c82e91b0d78e788`.
Work remains on `sync/upstream-2026-09-02` and has not been merged to `main`.

## Continuation Run 3

- Target branch: `sync/upstream-2026-09-02`
- Previously reviewed baseline: `c3cf80c0d1be43f3b84e21ef2c82e91b0d78e788`
- Reviewed upstream head: `003826320f320dace212a67e82b0db92cb457081`
- Live upstream head queried: `003826320f320dace212a67e82b0db92cb457081`
- Query time: `2026-09-03T00:56:23+02:00`
- Reviewed range: `c3cf80c0..00382632`

This continuation reviewed the remaining 17 commits from the previous
baseline. Counts: 1 A, 4 B, and 12 C. The reviewed baseline is now
`003826320f320dace212a67e82b0db92cb457081`; no upstream commits remain in
the queried range.

### Decisions

| Upstream | Class | Local commit | Disposition                                                                                                                                             |
| -------- | ----- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 8514ce3b | C     | --           | Broad language-model stream/schema rewrite conflicts across ZZZ's provider APIs and deleted test surfaces; not isolatable in this batch.                |
| 2b0562a8 | A     | 68a0d618     | Cherry-picked with `-x -s`; keybinding labels can be hidden while bindings remain active.                                                               |
| ff68a64c | B     | f5787a4f69   | Version follow to v1.20.0; local version bump retained.                                                                                                 |
| cff4edce | C     | --           | Nix/Corgi build-source plumbing with no independent ZZZ product behavior.                                                                               |
| 810c6a04 | B     | 7c88b253     | Added the `on_new_window` setting and launchpad behavior, adapting the settings UI to ZZZ's localized page-data API.                                    |
| 769d0bef | C     | --           | Emmet wrap-with-abbreviation requires a large new inline-input/protocol architecture and collab changes absent from ZZZ.                                |
| 6b5e15ed | C     | --           | Dependency, lockfile, and build compatibility churn without an independent ZZZ behavior.                                                                |
| 9decdcc1 | C     | --           | GPUI web IME mirror-focus rewrite depends on the divergent touch/IME architecture.                                                                      |
| 60bf47b9 | C     | --           | Project-scoped LSP log identity requires a broad `LogStore`/extension API rewrite not isolatable onto ZZZ's current model.                              |
| a85cf449 | C     | --           | Touch prediction jitter fix targets upstream `gpui/src/gestures.rs`, which ZZZ has deleted.                                                             |
| f8000a30 | B     | ea316783     | Cleared stale per-server diagnostics and inlay state, resolving the closed-buffer sweep against ZZZ's existing cleanup path.                            |
| ad251e0a | B     | ce366c3b     | Documented `focus_follows_mouse`, adapting placement and preserving ZZZ's existing format-on-save anchor.                                               |
| ab7d21d4 | C     | --           | Workspace-diagnostics polling overhaul conflicts with ZZZ's local diagnostics implementation and requires a large unisolated LSP error/request rewrite. |
| 8a4bd132 | C     | --           | Release-bundle symbol stripping and Sentry/debug-file packaging are upstream release infrastructure.                                                    |
| 23aca989 | C     | --           | Dynamic LSP document-selector support is a broad capability/protocol rewrite across divergent ZZZ APIs.                                                 |
| b3326e13 | C     | --           | `TouchDragEvent` depends on the absent upstream touch-gesture API.                                                                                      |
| 00382632 | C     | --           | Manage-skills command filtering targets an action surface absent from ZZZ's ACP-only agent UI.                                                          |

### Applied work

- `2b0562a8` was absorbed directly with `git cherry-pick -x -s`.
- `810c6a04`, `f8000a30`, and `ad251e0a` were ported as B commits with
  `Upstream`, `Retained`, and `Omitted` trailers. The settings UI, diagnostics
  cleanup, and documentation hunks were adapted to ZZZ's existing APIs.
  The incompatible inlay-hints fixture was omitted in follow-up commit
  `39ff921d`.

### Verification

```text
PASS cargo check --locked -p ui -p settings -p settings_ui -p workspace -p zzz -p project -p editor -p lsp
PASS cargo test --locked -p ui --lib (68 passed)
PASS cargo test --locked -p editor test_no_hint_updates_for_unrelated_language_files
BLOCKED cargo test --locked -p project diagnostic (existing integration fixtures use unavailable DiagnosticMessage/DiagnosticEntry::new and mismatched LanguageMatcher types)
PASS git diff --check
NOT RUN macOS / Windows / wasm32 runtime tests
NOT RUN cargo test --workspace
```

The reviewed baseline is `003826320f320dace212a67e82b0db92cb457081`.
Work remains on `sync/upstream-2026-09-02` and has not been merged to `main`.

## Continuation Run 4

- Target branch: `sync/upstream-2026-09-02`
- Previously reviewed baseline: `003826320f320dace212a67e82b0db92cb457081`
- Reviewed upstream head: `5f2d7ad735c266854503c1f40e9b490a8fd0e3a0`
- Live upstream head queried: `5f2d7ad735c266854503c1f40e9b490a8fd0e3a0`
- Query time: `2026-09-03T01:04:49+02:00`
- Reviewed range: `00382632..5f2d7ad7`

This short continuation reviewed one newly published commit. Counts: 0 A,
0 B, and 1 C. The reviewed baseline is now
`5f2d7ad735c266854503c1f40e9b490a8fd0e3a0`; no upstream commits remain in
the queried range.

### Decisions

| Upstream | Class | Local commit | Disposition                                                                                                                                             |
| -------- | ----- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 5f2d7ad7 | C     | --           | Extension CLI validation depends on the unabsorbed `language::QueryFile` parser; no safe isolated equivalent exists in ZZZ's current extension tooling. |

### Applied work

No product changes were applied.

### Verification

```text
PASS git merge-base --is-ancestor 00382632 FETCH_HEAD
PASS git diff --check
NOT RUN cargo check (no product changes)
NOT RUN macOS / Windows / wasm32 runtime tests
NOT RUN cargo test --workspace
```

The reviewed baseline is `5f2d7ad735c266854503c1f40e9b490a8fd0e3a0`.
Work remains on `sync/upstream-2026-09-02` and has not been merged to `main`.
