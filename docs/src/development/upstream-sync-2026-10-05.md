---
title: Upstream Sync 2026-10-05
description: Review of the next 20 Zed commits after the October 4 baseline.
---

# Upstream Sync 2026-10-05

## Scope

- Target branch: continued `sync/upstream-2026-10-04` from
  `98f1d09eab93d45a41f72c8727fed7c88b654e82`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `f8c2cc844057540ca1eac7de4f19f50d7597dead`
- Reviewed head: `981b224a46f26d21f925d62f67473d2df832da7b`
- Live upstream head at batch selection: `a84689073d296dfd39987bc7dd478e43ef76d83a`
- Live upstream head at final refresh:
  `279fe070bb389b79652e52065b2f001edcc0b11b`
- Final query time: `2026-10-05T00:12:05Z`
- Reviewed range: the first 20 commits after `f8c2cc8440`, oldest first
- Remaining after this batch: 28 commits through the final live head

The batch remained fixed after selection. The live branch advanced by one commit
during the run; that new commit stays queued for the next review.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                               |
| ---------- | ----- | ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 74c134a3c1 | B     | 22d1616401   | Ignored single-file worktrees for Git initialization, hid the initialize button without a directory worktree, and kept all existing localized labels.                                     |
| 71456c40f3 | B     | 79caac6170   | Hid disabled which-key candidates through dispatch-equivalent resolution while preserving ZZZ's user-over-base `NoAction` precedence.                                                     |
| 4e76d6ef82 | C     | --           | Authenticated ChatGPT subscription catalog loading belongs to the absent commercial `openai_subscribed` provider.                                                                         |
| 06afd454ec | B     | 8615e8a9ab   | Removed the final unused archive target type left from the already-deleted agent-server extension manifest path.                                                                          |
| 588e7c5241 | B     | 0c3c9a864e   | Documented feature-derived extension categories with ZZZ terminology and the existing local publishing-prerequisites anchor.                                                              |
| 17d3378bb9 | C     | --           | Adds account-settings notifications to the cloud WebSocket protocol and account-bound consumers, which are outside ZZZ's no-account product.                                              |
| 1399a804bd | B     | 8d9af32fcb   | Ported extension `opt_in_languages` parsing, validation, wildcard exclusion, explicit enablement, tests, and ZZZ-branded documentation.                                                   |
| 95cd535a5f | C     | --           | Production-wide GPUI `test-support` infrastructure has no current ZZZ caller, adds unused public test APIs, and includes the absent `gpui_apple` path.                                    |
| 7bbd162ddf | C     | --           | Adds a context-menu spacing API without a current ZZZ caller; importing it would be unused scaffolding.                                                                                   |
| cd4fc8de4c | C     | --           | Broad platform frame journaling depends on the absent GPUI hang-profiler and journal architecture.                                                                                        |
| 506beb34de | C     | --           | Release-build selector optimization depends on the rejected production `test-support` change and has no current ZZZ production path.                                                      |
| 36b6d0951f | C     | --           | Follow-up interval-sealing logic depends on rejected `cd4fc8de4c` and the absent hang-profiler journal.                                                                                   |
| 20d29fc6bc | B     | 6b6d31fc80   | Ported stale non-recursive watch repair and watch-before-enumeration onto ZZZ's current `GlobalWatcher` and worktree scanner.                                                             |
| 57bfce2945 | B     | ec5cbc9268   | Enabled split directory multi-diffs, propagated late-loaded languages, and retained ZZZ's localized title and hidden diff controls.                                                       |
| 2a97fbf22b | B     | a972c53001   | Kept the localized ACP awaiting-confirmation label on one line so its animated ellipsis cannot change row height.                                                                         |
| 35bba8eef7 | B     | d866f2bf4c   | Kept alternate built-in Python and TypeScript language servers outside the wildcard set while preserving explicit selection.                                                              |
| ba2b48461d | C     | --           | Security-version churn is confined to the `update_top_ranking_issues` automation lockfile and changes no shipped ZZZ behavior.                                                            |
| 07310d0f9d | A     | e1eaba6c0f   | Clean documentation absorption: tab-switcher instructions now use action-derived keybindings for Toggle All, Confirm, and Cancel.                                                         |
| 651d693d42 | C     | --           | Depends on upstream's path-based multi-selection, directory-mark coverage, and selection-target architecture; ZZZ only has an unused index-mark vector, so the feature is not isolatable. |
| 981b224a46 | A     | ea9696d3e8   | Clean absorption: forward Delete now has a distinct icon and regression coverage across platform styles.                                                                                  |

Totals: two `A`, nine `B`, nine `C`.

## Applied Work

### `74c134a3c1` single-file Git initialization

`22d1616401` filters single-file worktrees before choosing a `git init` target.
The Git panel still shows its localized empty-repository label, but only offers
the initialize action when a directory worktree exists. A visual-context test
covers the command path and verifies that no `.git` directory is created below
the file.

### `71456c40f3` disabled pending keybindings

The direct patch applied, but one new test exposed that ZZZ still had an older
`NoAction` source-precedence implementation. `79caac6170` therefore ports the
which-key filtering as a `B`: pending candidates resolve through the dispatch
path, base-keymap null bindings suppress weaker defaults, and stronger user
bindings remain visible.

### `06afd454ec` agent extension cleanup

ZZZ had already removed the agent-server manifest entry, proto conversions, and
proto dependency. `8615e8a9ab` removes the remaining unused `TargetConfig` type
without restoring any agent-server extension machinery.

### `588e7c5241` extension categories

`0c3c9a864e` explains that `extension.toml` features determine Extensions-page
categories and that one extension may appear in several categories. It links to
ZZZ's in-page publishing prerequisites instead of upstream's separate hosted
documentation path.

### `1399a804bd` extension language-server opt-in

`8d9af32fcb` adds `opt_in_languages` to extension manifests, validates that its
entries are also listed in `languages`, and removes opt-in adapters from the
`"..."` wildcard while still honoring explicit names. The port uses ZZZ's
current per-language adapter API and omits upstream's future schema-v2 test
scaffold.

### `20d29fc6bc` stale directory watches

`6b6d31fc80` marks Linux-style non-recursive native registrations stale after
directory removal, rename, or watcher overflow. The next add reinstalls the OS
watch once for all subscribers. Backend mutations remain serialized with
registration state, and worktree directory scans install their watch before
enumerating children to close the event-loss window.

### `57bfce2945` split directory diffs

`ec5cbc9268` moves directory `--diff` views onto `SplittableEditor`, follows the
configured unified or split style, forwards editor events, and copies languages
that load late from right-hand buffers to their base buffers. ZZZ keeps its
localized tab title, temporary-diff mode, disabled diagnostics, and hidden hunk
controls.

### `2a97fbf22b` confirmation label layout

`a972c53001` applies `single_line()` to the existing localized ACP
awaiting-confirmation label, preventing the animated ellipsis from changing the
tool row height.

### `35bba8eef7` built-in language-server defaults

`d866f2bf4c` marks `ty`, Pyright, PyLSP, and the alternate TypeScript server as
opt-in for their applicable languages. Documentation now distinguishes servers
excluded from `"..."` from older settings-only default exclusions.

The two direct `A` changes, `07310d0f9d` and `981b224a46`, used
`git cherry-pick -x -s`.

## Rejected Work

- `4e76d6ef82` targets the absent authenticated ChatGPT subscription provider.
- `17d3378bb9` extends cloud account settings and account-bound consumers.
- `95cd535a5f` prepares GPUI `test-support` for production consumers that ZZZ
  does not have and crosses the absent `gpui_apple` crate.
- `7bbd162ddf` is a new public UI API with no current caller.
- `cd4fc8de4c` and `36b6d0951f` require the deleted GPUI hang-profiler journal.
- `506beb34de` only matters after the rejected production `test-support` change.
- `ba2b48461d` is an automation-only Python lockfile update.
- `651d693d42` assumes a newer Git-panel multi-selection model that ZZZ lacks;
  recreating it would be a broad prerequisite rewrite rather than an isolated
  file-action fix.

## Verification

| Check                                                                                                                                                                                | Result  |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------- |
| Final live `git ls-remote` and `git fetch --no-tags` to `FETCH_HEAD`                                                                                                                 | PASS    |
| Previous baseline and reviewed head are ancestors of final `FETCH_HEAD`                                                                                                              | PASS    |
| `git diff --check`                                                                                                                                                                   | PASS    |
| `cargo fmt --all -- --check`                                                                                                                                                         | PASS    |
| `cargo check --locked` for `agent_ui`, `extension`, `extension_cli`, `fs`, `git_ui`, `gpui`, `icons`, `language`, `language_extension`, `languages`, `project`, `ui`, and `worktree` | PASS    |
| Git-init single-file regression                                                                                                                                                      | PASS    |
| GPUI keymap and possible-next-binding regressions                                                                                                                                    | PASS    |
| Extension manifest and CLI opt-in validation regressions                                                                                                                             | PASS    |
| Project opt-in language-server startup regression                                                                                                                                    | PASS    |
| File-watcher stale-registration suite and worktree watch-before-read regressions                                                                                                     | PASS    |
| Unified and split directory multi-diff regressions                                                                                                                                   | PASS    |
| Forward Delete icon regression                                                                                                                                                       | PASS    |
| Prettier for all documentation changed in this batch                                                                                                                                 | PASS    |
| English / Simplified Chinese recursive locale key-set comparison                                                                                                                     | PASS    |
| `./script/check-philosophy`                                                                                                                                                          | PASS    |
| `./script/backfill-upstream-ledger` and `./script/check-upstream-ledger`                                                                                                             | PASS    |
| macOS / Windows runtime checks                                                                                                                                                       | NOT RUN |
| `cargo test --workspace`                                                                                                                                                             | NOT RUN |

The general reviewed baseline advances to
`981b224a46f26d21f925d62f67473d2df832da7b`.
