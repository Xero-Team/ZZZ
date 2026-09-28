---
title: Upstream Sync Audit 2026-09-28
description: Pre-audit gap scan for missed Zed upstream commits (May–June 2026).
---

# Upstream Sync Audit 2026-09-28

## Scope

- Local base: `sync/upstream-2026-09-28-backport-hide-button` at `d16b1fd8af`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Upstream reference used: `bda9c0bd43a8d235d82adb01ea5bc875b861ecfc`
- Gap window scanned: `2026-05-01` through `2026-06-20`
- Reviewed baseline: unchanged at `bda9c0bd43`

The first recorded sync report (`upstream-sync-2026-07-03.md`) set a
candidate range boundary of `2026-06-20`. Upstream commits before that
boundary were never classified, and the local `main` import history for
May 2026 is ad-hoc rather than audited. The missed
`c049193fd93b2c27d3e4a8a40f5e1739db4640b1` (PR #54971) sits in this
window, so this audit scans the whole pre-boundary gap for similar
oversights.

## Method

Upstream commits were enumerated from `bda9c0bd43`, then matched against
the local `main` history with four independent signals:

1. PR number `(#NNNNN)` in the subject.
2. Normalized subject (PR number stripped, lowercased).
3. `git patch-id --stable` equality.
4. Verbatim presence of added code lines in the local tree.

A commit with no signal from any of the four is a candidate. Because ZZZ
rewrites subjects for `B` ports and conflict-resolves patches, and because
some features were independently present, the candidates were then
spot-checked against the local source. This scan is a lead generator, not
a substitute for a per-commit diff read.

## Results

- Upstream commits dated `2026-04-01`..`2026-06-20`: `1972`.
- Candidates with no signal from any method: `638`.
- After discarding philosophy-rejected topics (agent, skills, sandbox,
  edit prediction, collab, cloud, models, docs, CI/release), high-confidence
  candidates: `88`.
- Confirmed genuine misses after source inspection: `3` (one already
  ported, two open).

### Confirmed misses

| Upstream | Subject                                                                       | Status                                         |
| -------- | ----------------------------------------------------------------------------- | ---------------------------------------------- |
| c049193f | Make all status bar tools able to hide its button via UI (#54971)             | Ported as `2de3161a49`                         |
| 0711d9e4 | git_ui: Fix stale loading message in git history pane (#58133)                | Missing; `commit_history_shas` absent locally  |
| 5791ebc2 | git_ui: Fix Enter key selecting branch in commit modal branch picker (#58366) | Missing; no `GitCommit` keymap context locally |

### Candidates rejected or already present

Spot checks that returned present (heuristic false positives):
`dedb2af9` (#54708, `flex_none()` present), `3c0f5a04` (#58176,
`ArithmeticError` present), `fcb95aae` (#58614, markdown `~`
auto-surround present), `626dc16f` (#58269, `split_leading_icon_char`
present), `7e6e731b` (#58136, `list_splat_pattern` present),
`f99fe5d8` (#58741, `SHORT_SHA_LENGTH` present), and
`#54878` (`buffer_font_fallbacks` present).

Candidates rejected as philosophy or absent-architecture:

- Agent, native-agent, skills, rules, sandbox, compaction, ACP UI, and
  edit-prediction commits (the ZZZ boundary keeps these ACP-only and
  minimal).
- Collab, cloud, account, organization, billing, and Zed Business
  commits, including `216980f6` (#57316 `OpenStatusPage`).
- `4eab0696` (#58341) and `4e949a1c` (#58325): the local
  `mermaid_render` tree and `merman` dependency rev have diverged, so the
  fix is not directly portable.
- Release, CI, dependency-bump, docs, and community-workflow commits.

## Verification

| Check                                        | Result |
| -------------------------------------------- | ------ |
| `git status --porcelain` (clean before scan) | PASS   |
| Upstream enumeration from reviewed head      | PASS   |
| Source spot checks for the confirmed misses  | PASS   |
| `./script/check-upstream-ledger`             | PASS   |

No product code changed in this audit beyond the already-landed
`2de3161a49` backport. macOS and Windows runtime behavior was `NOT RUN`.

## Recommendation

The gap window is dominated by philosophy-rejected and already-present
commits; the confirmed editor-visible misses are `#58133` and `#58366`.
A fully trustworthy "no commit missed" claim still requires reading each
of the `88` high-confidence candidates; that should be done as a normal
forward batch on the gap window if the user wants completeness.
