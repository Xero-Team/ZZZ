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
