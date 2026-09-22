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

### F3: Over-rejection candidates not yet re-reviewed

Roughly 50 `C` decisions are justified only by "conflict", "already
diverged", or "not isolatable", without recording an isolation attempt. The
skill treats a dry-run conflict as evidence for `B` or `C`, decided by
isolation, so these are candidates for a future pass. Spot checks that look
like isolatable, philosophy-safe bugfixes or features:

- `83dc1967` worktree ignore-rule anchoring
- `a4916265` OpenAI reasoning-summary separators
- `0ad5441b` display-position selection alignment
- `a034d870` canceled-worktree load leak
- `4bd19937` Mermaid `~~~` fences
- `c6b01d8a` optional git stash message
- `6634c945`, `bd1b83a4` git panel copy-path and collapsible sections

This pass did not port them; they remain recorded as `C`.

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
| `REFERENCE.md`                | Updated `LAST_REVIEWED_UPSTREAM`, `LOCAL_BASE_COMMIT`, and the baseline prose                                                                     |

No product code changed in this pass.

## Verification

```text
PASS git diff --check
PASS no report lost decision rows relative to its historical union
PASS restored batch 1 rows match commit 35b4e80a9d
PASS local commits referenced by restored rows exist on main
PASS cd docs && npx prettier --check src/development/
NOT RUN cargo tests (record-only changes)
NOT RUN macOS / Windows / wasm32 runtime tests
```
