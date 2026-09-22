---
title: Upstream Sync Re-audit 2026-09-22
description: Re-audit of past Zed upstream absorption records.
---

# Upstream Sync Re-audit 2026-09-22

## Scope

- Repository: ZZZ, local `main` at
  `a8be04d04f7471b8aef85b6f84658e5c834b3808`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Audited records: `docs/src/development/upstream-sync-2026-07-03.md`
  through `upstream-sync-2026-09-18.md`
- Audited decision rows: 744 (149 `A`, 197 `B`, 398 `C`) across nine reports

Method: cross-reference every recorded `C` against later local `sync:`
commits, compare each report against its first committed revision, and
re-check the version-follow policy against the current app version.

## Findings

### F1: The 2026-09-18 report lost its first batch

`sync/upstream-2026-09-18` covered `490aad88..b961b495` (171 commits), but a
later batch overwrote the report's scope and decision table. The first batch,
`490aad88..d7f28899` (20 commits, originally 5 `A`, 7 `B`, 8 `C`), lost all
of its decision rows and narrative. The surviving counts were short by 20
(151 instead of 171), the scope baseline said `0d08af1e` instead of
`490aad88`, and every `## Batch N` section was offset by one.

The rows and narrative were recovered from commit `35b4e80a9d` and restored.
All referenced local commits still exist on `main`.

### F2: Version-follow commits were classified inconsistently

Eight "Bump Zed to vX.Y.0" commits were reviewed. The policy set by
`b13f6c71` (v1.17.0) and `b0db8327` (v1.22.0) is to follow the upstream app
version as `B`, but the others were recorded as `C` "release metadata", and
two of them were later absorbed without updating the record. The current
`crates/zzz` version is `1.22.0`.

| Upstream | Version | Was | Now | Local commit               |
| -------- | ------- | --- | --- | -------------------------- |
| e24eeb71 | v1.15.0 | C   | B   | superseded by `209a6048`   |
| 02c6dd95 | v1.16.0 | C   | B   | superseded by `209a6048`   |
| b13f6c71 | v1.17.0 | B   | B   | `209a6048`                 |
| 0a4a4a95 | v1.18.0 | C   | B   | superseded by `36ec8c40d7` |
| ac099b4a | v1.19.0 | C   | B   | `36ec8c40d7`               |
| ff68a64c | v1.20.0 | C   | B   | `f36e6093a5`               |
| 71b60bba | v1.21.0 | C   | B   | superseded by `57b5799a`   |
| b0db8327 | v1.22.0 | B   | B   | `57b5799a`                 |

Superseded versions carry no separate local commit because a later follow
already moved `crates/zzz` past them.

### F3: Over-rejection re-review

Roughly 60 `C` decisions are justified only by "conflict", "already
diverged", or "not isolatable", without recording an isolation attempt. The
skill treats a dry-run conflict as evidence for `B` or `C`, decided by
isolation. Each candidate was re-checked against the current ZZZ call chain.

Ten were genuinely missed and are now ported:

| Upstream   | Class | Local commit | What was missed                               |
| ---------- | ----- | ------------ | --------------------------------------------- |
| `83dc1967` | B     | `7b4c1a46f9` | Worktree `info/exclude` anchoring bugfix      |
| `1e3d8b5a` | B     | `fb304e06b8` | O(files^2) project-search snapshot clone      |
| `a4916265` | B     | `89c1511729` | Missing separator between reasoning summaries |
| `992c7d46` | B     | `4a20d45f0a` | Unbounded GitHub release requests             |
| `30f806c4` | B     | `d3dad3d954` | JetBrains CamelHump subword keybindings       |
| `47825fe0` | B     | `ba686f3ca0` | Invisible-character replacement width         |
| `f543a764` | A     | `81a751b591` | Invisible-character range matching            |
| `c6b01d8a` | B     | `a3c4dd2111` | Optional git stash message                    |
| `6634c945` | B     | `2260d67f9b` | Git panel copy path actions                   |
| `bd1b83a4` | B     | `b4c301e099` | Git panel collapsible sections                |

Two were re-checked and confirmed correctly `C`, so they were not ported:

- `c83adb3d` `SymbolKind` RPC: local `language_core` uses `lsp::SymbolKind`
  directly, so the serialization bug does not exist.
- `dbf7f638` Anthropic compaction context: `into_compact_request` has no
  ZZZ caller; explicit compaction is only wired into rejected native-agent
  and cloud paths.

The remaining candidates are philosophy-safe but need per-commit adaptation
and were not ported in this pass:

- `4bdf188c` stash tracked/staged options
- `e4671f71` malformed-`tasks.json` error toast (settings-observer refactor)
- `6721ea2e` reset dock panels, `0ad5441b` display-position selections
- `a034d870` canceled-worktree leak, `d4010e91` trash-confirm undo
- `897ba9ad`, `eb548352`, `c1eda3e8` markdown preview

The large architecture rewrites stay `C`: web IME/touch, MultiWorkspace,
project search/scanner, LSP protocol, outline panel, language-model stream
unification, and build/bloat work.

### F4: The skill reference baseline was stale

`REFERENCE.md` still named the 2026-08-18 audit through `aa3718614b`. It now
names the 2026-09-18 audit through `b961b495`.

## Corrections applied

| File                          | Correction                                                                                                                                        |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `upstream-sync-2026-09-18.md` | Restored the 20 lost batch 1 rows and narrative; fixed scope, range, totals (35 A, 46 B, 90 C), and batch numbering; reclassified `71b60bba` to B |
| `upstream-sync-2026-08-07.md` | Reclassified `e24eeb71` and `02c6dd95` to B version follows                                                                                       |
| `upstream-sync-2026-08-27.md` | Reclassified `0a4a4a95` and `ac099b4a` to B; corrected batch counts and rejected-work notes                                                       |
| `upstream-sync-2026-09-02.md` | Reclassified `ff68a64c` to B; corrected batch count                                                                                               |
| `upstream-sync-2026-08-18.md` | Reclassified the ten F3 ports with their local commits; corrected counts to 29 A, 17 B, 70 C                                                      |
| `REFERENCE.md`                | Updated `LAST_REVIEWED_UPSTREAM`, `LOCAL_BASE_COMMIT`, and the baseline prose                                                                     |

The record-only corrections changed no product code. The F3 re-review added
ten local commits (`7b4c1a46f9`, `fb304e06b8`, `89c1511729`, `4a20d45f0a`,
`d3dad3d954`, `ba686f3ca0`, `81a751b591`, `a3c4dd2111`, `2260d67f9b`,
`b4c301e099`).

## Verification

```text
PASS git diff --check
PASS no report lost decision rows relative to its historical union
PASS restored batch 1 rows match commit 35b4e80a9d
PASS local commits referenced by restored rows exist on main
PASS cd docs && npx prettier --check src/development/
PASS cargo check --locked -p worktree -p project -p open_ai -p reqwest_client -p http_client -p git -p fs -p git_ui
PASS cargo test --locked -p worktree --test integration test_repo_exclude
PASS cargo test --locked -p open_ai responses_stream_separates_reasoning_summary_items
PASS cargo test --locked -p reqwest_client test_request_timeout_applies_while_reading_response_body
PASS cargo test --locked -p editor --lib wrap_map::tests::test_invisibles
PASS cargo test --locked -p git_ui --lib test_copy_paths
PASS ./script/check-keymaps
NOT RUN cargo test --workspace
NOT RUN macOS / Windows / wasm32 runtime tests
```
