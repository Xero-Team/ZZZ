---
title: Upstream Sync 2026-10-13
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-10-13

## Scope

- Target branch: `sync/upstream-2026-10-13`, branched from `main` at
  `f3832cb0ad`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `7eccdcecb5c5fe150a9e94f98b55af30b7d486b6`
- Reviewed head: `8d541bcc9bddf9c6e1b1d07e4be9e8c798cc838c`
- Live upstream head queried: `8d541bcc9bddf9c6e1b1d07e4be9e8c798cc838c`
- Query time: `2026-10-13T11:32:06Z`
- Reviewed range: the eleven commits after `7eccdcecb5`, oldest first
- Remaining after this batch: 0 commits through the queried live head

The previous reviewed baseline is an ancestor of `FETCH_HEAD`. The range is
shorter than the twenty-commit batch because upstream advanced by only eleven
commits since the last query.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                             |
| ---------- | ----- | ------------ | ----------------------------------------------------------------------------------------------------------------------- |
| 6802ba72f0 | B     | 69c873a19a   | Jumps reveal folded destinations; navigation.rs hunks re-homed into ZZZ's consolidated editor.rs.                       |
| 4905e67333 | C     | --           | Fixes a permission in the `welcome_first_contribution.yml` community workflow; ZZZ ships no such automation.            |
| f929b3f7ce | C     | --           | Reimplements `PathStyle::strip_prefix` on the absent `crates/path` `Components` API; unisolatable.                      |
| 725573ec14 | C     | --           | Native-agent session release and cancel lifecycle in `crates/agent`; ZZZ keeps no native-agent surface.                 |
| c66312b200 | C     | --           | ACP v2 shared receipt-lifecycle scaffolding; ZZZ pins schema v1 and has no v2 transport.                                |
| ff52936d53 | C     | --           | Decouples process output onto a v2 exit-status snapshot and native handles; needs the unadopted v2 schema.              |
| dd85cf3677 | C     | --           | Renames shared `Meta` aliases from v1 to v2 spelling; stacked on unabsorbed v2 work and ZZZ's `acp` alias is schema v1. |
| 735bc10fd0 | C     | --           | Moves local terminal environments to `(String, String)` pairs through native `agent` thread APIs.                       |
| 10644801ad | C     | --           | Normalizes mode and default APIs onto v2 config select-choice types absent from ZZZ.                                    |
| 26363aabdb | C     | --           | Relicenses the absent `extension_suggest` crate from GPL-3.0-or-later to Apache-2.0.                                    |
| 8d541bcc9b | B     | be7b28652d   | Docs link and anchor fixes ported onto ZZZ docs; philosophy-diverged and removed-content hunks omitted.                 |

Totals: zero `A`, two `B`, nine `C`.

## Applied Work

- `69c873a19a` is a `B` port of `6802ba72f0`. An opt-in
  `SelectionEffects::unfold` flag reveals folded destinations before scrolling
  for go-to-line, definition/declaration/implementation jumps, symbol
  navigation, outline and project-symbol pickers, multibuffer location
  results, and navigation history, leaving unrelated folds intact. Upstream
  edited `crates/editor/src/navigation.rs`, which ZZZ has consolidated into
  `crates/editor/src/editor.rs`, so those hunks were re-homed onto the current
  `editor.rs` call sites. `resolve_selections_wrapping_blocks` is used in place
  of upstream's `resolve_selections`, and `Anchor::buffer_id()` is replaced by
  `raw_text_anchor().buffer_id` because ZZZ's `multi_buffer::Anchor` lacks that
  helper. Both upstream regression tests were kept.
- `be7b28652d` is a `B` port of `8d541bcc9b`. Link and anchor fixes landed for
  key bindings, the command palette, Tailwind CSS, the Emmet comment filter,
  Gleam and Roc language-server paths, extension repository URLs, the
  `use_autoclose` anchor, the new `modeline_lines` settings entry, and the
  `crates/grammars` paths. The git permalink section was rewritten with ZZZ
  branding.

## Rejected Work

- `4905e67333` narrows a GitHub Actions permission on
  `.github/workflows/welcome_first_contribution.yml`, a community automation
  workflow ZZZ does not ship (its introducing commit `fa1b94c6b9` was rejected
  in `upstream-sync-2026-10-12.md`).
- `f929b3f7ce` rewrites `PathStyle::strip_prefix` on top of the `crates/path`
  `Components` API and updates callers across `agent`, `agent_ui`, `project`,
  and `workspace`. ZZZ has no `crates/path` crate; its `util::paths::PathStyle`
  is a `&str`-based implementation without a cross-platform `Components`, so
  the refactor cannot be isolated.
- `725573ec14` retains weak ACP-thread handles and cancels native work when a
  `NativeAgent` session is released. ZZZ keeps no native-agent surface.
- `c66312b200` prepares shared ACP v2 receipt lifecycle support with 72 `acp_v2`
  references; `ff52936d53` decouples process output onto a v2 exit-status
  snapshot; `dd85cf3677` renames shared `Meta` aliases to the v2 spelling;
  `735bc10fd0` routes local terminal environments through native `agent` thread
  APIs; `10644801ad` normalizes modes and defaults onto v2 config select-choice
  types. All five build on the unadopted ACP v2 schema (ZZZ pins
  `agent-client-protocol` schema v1) and are stacked on the rejected native
  commit `725573ec14`.
- `26363aabdb` relicenses `crates/extension_suggest`, which ZZZ does not have.

## Verification

| Check                                                                               | Result  | Notes                                                                          |
| ----------------------------------------------------------------------------------- | ------- | ------------------------------------------------------------------------------ |
| Live `git ls-remote` and `git fetch --no-tags`                                      | PASS    | `FETCH_HEAD=8d541bcc9bddf9c6e1b1d07e4be9e8c798cc838c`                          |
| Previous baseline ancestry                                                          | PASS    | `7eccdcecb5` is an ancestor of `FETCH_HEAD`                                    |
| `cargo check --locked -p editor`                                                    | PASS    | Unfold port compiles                                                           |
| `cargo check --locked -p go_to_line -p outline -p outline_panel -p project_symbols` | PASS    | Call-site ports compile                                                        |
| `cargo check --locked --tests -p editor -p go_to_line`                              | PASS    | Ported regression tests compile                                                |
| `cargo test --locked -p editor unfold`                                              | PASS    | `test_selection_effects_unfold_opt_in`, `test_jumps_unfold_enclosing_sections` |
| `cargo test --locked -p go_to_line`                                                 | PASS    | Nine tests pass, including `test_go_to_line_unfolds_enclosing_sections`        |
| `git diff --check` and `cargo fmt --check`                                          | PASS    | Only pre-existing `crates/project/src` formatting drift remains                |
| Docs Prettier check                                                                 | PASS    | All changed docs files use Prettier style                                      |
| Rejection ledger backfill and check                                                 | PASS    | `./script/backfill-upstream-ledger` and `./script/check-upstream-ledger`       |
| macOS and Windows runtime checks                                                    | NOT RUN | Host is macOS; platform-specific runtime behavior not exercised                |
| `cargo test --workspace`                                                            | NOT RUN | Not requested                                                                  |

The general reviewed baseline advances to
`8d541bcc9bddf9c6e1b1d07e4be9e8c798cc838c`, the queried live head.
