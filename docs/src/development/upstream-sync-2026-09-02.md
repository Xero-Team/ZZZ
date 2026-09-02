---
title: Upstream Sync 2026-09-02
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-09-02

## Scope

- Target branch: `sync/upstream-2026-09-02` from `main` at
  `864fc21e199b61b110a5be345171f4e890f894f0`
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
| a61e2609 | B     | 13fde019           | Normalize reversed LSP ranges on existing editor, project, document-color, and Copilot paths.                 |
| aa347c30 | B     | c889bc72, 33b16f39 | Preserve single-file breakpoint paths; adapt to local `RelPathBuf`.                                           |
| ca2b5741 | C     | --                 | New upstream lint/test infrastructure only; no independent product behavior.                                  |
| 2890c340 | C     | --                 | Upstream `.rules` instruction is repository scaffolding, not ZZZ product behavior.                            |
| e3adf43f | A     | caa3521d           | Cherry-picked with `-x -s`.                                                                                   |
| 1662f5f3 | B     | f227c16b, e669a29f | Accept full SHA-256 commit searches and add the local hash-length constant.                                   |
| 31a32671 | A     | 4fcbff03           | Cherry-picked with `-x -s`.                                                                                   |
| 82aeef2d | A     | 4e4e71e7           | Cherry-picked with `-x -s`.                                                                                   |
| 9dbc9e76 | A     | --                 | Already equivalent: the migration is already absent and `m_2025_06_27` already uses command shape detection.  |
| 19e458b7 | B     | bbd6edb2           | Include selected scopes in MCP OAuth dynamic client registration.                                             |
| 399258fe | B     | 72081642           | Reduce closure-funnel monomorphization in existing editor/settings paths.                                     |
| 68b65045 | C     | --                 | Bundle scripts have diverged substantially; Rust bootstrap/config plumbing is not isolatable.                 |
| 770aac34 | C     | --                 | Hosted `cloud_api_client` websocket change is outside ZZZ's local-first boundary.                             |
| 9a76a39d | B     | 9d6ce198           | Underline diagnostics that span only a line terminator; adapt `Unclipped<PointUtf16>`.                        |
| bce0c578 | B     | a3a1f934           | Recognize `.bash_login` as a Shell Script filename in a local sync commit.                                    |

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
| 24254879 | B     | 2fd39ac4                               | Filter editorconfig resolution to the keys ZZZ consumes and return `None` when no config exists; omit the divergent buffer cache and benchmark plumbing. |
| 7aa903ef | C     | --                                     | Adds keybindings for the deleted `tabular_data_preview` product surface.                                                                                 |
| adc72a5a | C     | --                                     | CSV preview support requires the absent tabular preview crate and related project-less architecture.                                                     |
| ef075910 | C     | --                                     | Web font ownership rewrite is coupled to upstream examples and has no independent ZZZ web caller; local examples are intentionally removed.              |
| 98c6c140 | B     | fb360c11, dd1df8d4                     | Gate synchronous blocking APIs on wasm and run quit handlers asynchronously; adapt to ZZZ's scheduler naming.                                            |
| a60addb9 | C     | --                                     | objc2 prompt migration conflicts with ZZZ's macOS platform layout and dependency feature set; no safe isolated port was established on this Linux host.  |
| e8fbacb6 | C     | --                                     | Large outline-panel rewrite is not isolatable from ZZZ's divergent panel implementation.                                                                 |
| ff020dd0 | B     | 453b18f3                               | Batch ordered conflict-anchor conversion and use it for conflict highlighting; omit upstream benchmark-generator tooling and duplicate tests.            |
| 81df6f4a | C     | --                                     | Replaces crash user data with Sentry tags, which is telemetry machinery rejected by ZZZ.                                                                 |
| 3f00b5d7 | B     | 6056ffda, c8a46208, 528970b4, d3af7201 | Restrict row-highlight expansion to the viewport and skip header rows; adapt stored-color and block visibility APIs.                                     |
| 76b1096c | C     | --                                     | Requires an unabsorbed gesture/prediction architecture and divergent GPUI web input APIs.                                                                |
| ee6badf4 | C     | --                                     | Corgi build support is upstream build/release infrastructure with no ZZZ product behavior.                                                               |
| 9785475c | C     | --                                     | Terminal Threads title editing belongs to the rejected native-agent/thread surface.                                                                      |
| ce48461e | A     | cc87f2bd                               | Cherry-picked with `-x -s`; malformed shell-variable references now pass through without panicking.                                                      |
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
