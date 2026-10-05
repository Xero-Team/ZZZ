---
title: Upstream Sync 2026-10-05
description: Review of two 20-commit batches and a final 8-commit Zed batch after the October 4 baseline.
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

## Continuation Batch

### Scope

- Target branch: continued `sync/upstream-2026-10-04` from
  `1114d6fb4f4cd794a027b0580d444a02dcf7b5b1`
- Previous reviewed baseline: `981b224a46f26d21f925d62f67473d2df832da7b`
- Reviewed head: `3fec4830142a48c08a0e9b8aa241eeaa1a261370`
- Live upstream head at selection and final refresh:
  `279fe070bb389b79652e52065b2f001edcc0b11b`
- Final query time: `2026-10-05T02:14:52Z`
- Reviewed range: the first 20 commits after `981b224a46`, oldest first
- Remaining after this batch: 8 commits through the final live head

The reviewed baseline and reviewed head are both ancestors of the final
`FETCH_HEAD`.

### Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                       |
| ---------- | ----- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| c9daddfa0a | B     | a9b06cf9f9   | Showed file counts for ZZZ's existing Conflicts, Tracked, and Untracked Git-panel sections by reusing their current counters instead of importing upstream staging-state grouping.                |
| 54edefa083 | C     | --           | Reorders the built-in native-agent profile tool inventory; ACP sessions receive tools from external agents, and ZZZ does not maintain this upstream in-tree tool catalog.                         |
| 28d5832adb | B     | d0fc10afa7   | Added `markdown_preview.heading_font_weight`, preview rendering, schema, documentation, and tests; omitted the unabsorbed Settings Editor typography-section migration.                           |
| cd8c467418 | C     | --           | Adds schema completion for the same upstream native-agent built-in tool inventory and would preserve an absent tool catalog as public settings scaffolding.                                       |
| c5b6d4386e | B     | 004f980edc   | Scoped ACP permission prompts, actions, selections, and RPC cancellation to stable request IDs on ZZZ's existing `ToolCallStatus` model.                                                          |
| 5c5236ded8 | A     | --           | Already equivalent: the sorting comments added by rejected `54edefa083` were never introduced into ZZZ's default settings.                                                                        |
| 6708a25004 | A     | --           | Already equivalent: `windows-and-projects.md` already documents both open-behavior settings with ZZZ's actual `new_window` defaults.                                                              |
| 90731bdec0 | A     | b41edd4dca   | Clean absorption: corrected minimap thumb drag geometry and added short, long, minimum-thumb, and immovable-thumb regressions.                                                                    |
| 7362739f94 | B     | 79b7858cbc   | Added reusable pixel-snapped choice cards and moved localized ACP single/multi-select elicitations onto them; omitted unavailable GPUI Role/ARIA APIs.                                            |
| 3209c7d31f | C     | --           | Explicitly adds generic ACP v2 permission model/UI foundation without wire enablement; the new insertion API has no production caller in this commit.                                             |
| 309f91f564 | C     | --           | Migrates Copilot enterprise authentication and subscription settings, which belong to an intentionally absent provider path.                                                                      |
| 9dd6993e2e | A     | 5390ed6db1   | Clean absorption with the local lockfile result: deserialize `SharedString` directly through `SmolStr` and remove the now-orphaned `borsh` package.                                               |
| 23d10a4754 | C     | --           | Adds a typed ACP v2 streamed tool-content append API but explicitly adds no wire handler; production has no caller, so this is future scaffolding.                                                |
| f37989fbdf | C     | --           | Broad cross-platform headless/windowed GPUI rewrite whose new switching API is called only by its example and platform tests in this commit.                                                      |
| 57af58b3ad | B     | fd2910f0d3   | Kept the current ACP v1 transport and added live kind/choice revalidation so stale select, boolean, picker, and favorite controls cannot mutate replaced configuration.                           |
| c83abe7d0e | B     | fbfd5b772b   | Added a localized, persisted, setting-backed collapsible Git commit editor; omitted Git Graph detail disclosures tied to the separately split and substantially diverged local `git_graph` crate. |
| 199500e9be | A     | --           | Already equivalent: ZZZ deleted the old async `file_content` detector; the current content-analysis path has no 64 KiB stack buffer.                                                              |
| 7ea5428f0c | C     | --           | Targets the absent in-tree Amazon Bedrock provider and `language_models` integration; rebuilding those deleted provider crates is outside this isolated commit.                                   |
| 708c7eef55 | B     | c41114e8a4   | Pre-filled File Finder from the focused editor or terminal selection, normalized and capped the query, and added a localized opt-out setting and regressions.                                     |
| 3fec483014 | C     | --           | First of an eight-part tabular-selection series, deliberately disabled in release builds until later copy/menu work; importing it alone would be staged future scaffolding.                       |

Totals: five `A`, seven `B`, eight `C`.

### Applied Work

#### Git panel section counts

`a9b06cf9f9` uses the current conflict, tracked, and untracked counters to
render muted count chips beside localized section headers. The upstream
staging-state section model remains absent.

#### Markdown preview heading weight

`d0fc10afa7` adds the nested `markdown_preview.heading_font_weight` setting
with a `600` default, applies it to H1-H6 only in preview typography, and
documents and tests the setting. ZZZ's older appearance-page structure has no
dedicated Markdown preview typography section, so no unused UI field was added.

#### Request-owned ACP permissions

`004f980edc` assigns every permission prompt a `PermissionRequestId`. Old RPC
cancellation, dropdown state, direct actions, and option selection now target
the exact request, and selected option kinds are validated against the options
the agent actually offered. The port keeps ZZZ's ACP-only status model instead
of restoring upstream native-agent state or telemetry.

#### Elicitation choice cards

`79b7858cbc` adds a shared `ChoiceCard` for radio and checkbox rows. Its radio
indicator uses pixel-snapped concentric quads, and both choice kinds retain
full-row keyboard/click interaction, validation styling, descriptions, and
localized elicitation text. GPUI's newer accessibility role APIs are not
available locally.

#### ACP configuration control hardening

`fd2910f0d3` leaves the working ACP v1 shared interface intact while ensuring
deployed selectors and pickers re-check the live option kind and choice list
before opening, selecting, confirming, toggling, or changing favorites.

#### Collapsible Git commit editor

`fbfd5b772b` adds `git::ToggleCommitEditor`, a localized chevron control,
workspace persistence, a `git_panel.commit_editor` default, Settings Editor
support, and documentation. Collapsing clears fill mode and prevents focus from
targeting the hidden editor.

#### File Finder selection seed

`c41114e8a4` reads the focused pane before the active center pane, requests the
current selection through the existing searchable-item interface, flattens
whitespace, caps the seed at 100 characters, and exposes a localized
`file_finder.prefill_query_from_selection` opt-out.

The two direct code absorptions, `90731bdec0` and `9dd6993e2e`, used
`git cherry-pick -x -s`. The three already-equivalent `A` commits required no
local commit.

### Rejected Work

- `54edefa083` and `cd8c467418` maintain upstream's in-tree native-agent tool
  catalog and its settings schema.
- `3209c7d31f` and `23d10a4754` explicitly add ACP v2 model/UI foundations
  without production wire callers.
- `309f91f564` restores Copilot enterprise authentication and settings.
- `f37989fbdf` is a broad GPUI platform rewrite for a switching API without a
  shipped ZZZ caller.
- `7ea5428f0c` depends on deleted Bedrock and in-tree language-model provider
  crates.
- `3fec483014` is release-disabled part-one scaffolding for an incomplete
  eight-commit tabular-selection series.

### Verification

| Check                                                                                                                                                                                                                               | Result                                                                                  |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| Final live `git ls-remote` and `git fetch --no-tags` to `FETCH_HEAD`                                                                                                                                                                | PASS                                                                                    |
| Previous baseline and reviewed head are ancestors of final `FETCH_HEAD`                                                                                                                                                             | PASS                                                                                    |
| `git diff --check` and `cargo fmt --all -- --check`                                                                                                                                                                                 | PASS                                                                                    |
| `cargo check --locked` for `git`, `git_ui`, `editor`, `gpui_shared_string`, `markdown`, `settings_content`, `theme_settings`, `acp_thread`, `agent_servers`, `agent_ui`, `ui`, `file_finder`, `open_path_prompt`, and `settings_ui` | PASS                                                                                    |
| Git-panel count and commit-editor collapse regressions                                                                                                                                                                              | PASS                                                                                    |
| Markdown preview heading-weight regressions                                                                                                                                                                                         | PASS                                                                                    |
| ACP request supersession, permission buttons/actions/granularity, and conversation ordering regressions                                                                                                                             | PASS                                                                                    |
| Elicitation choice keyboard accessibility and option-schema regressions                                                                                                                                                             | PASS                                                                                    |
| ACP stale configuration-control regression                                                                                                                                                                                          | PASS                                                                                    |
| Minimap thumb geometry regressions                                                                                                                                                                                                  | PASS                                                                                    |
| `gpui_shared_string` unit and doc tests                                                                                                                                                                                             | PASS                                                                                    |
| File Finder selection seed, opt-out, and query sanitization regressions                                                                                                                                                             | PASS                                                                                    |
| Elicitation text-field Enter and tab-navigation tests                                                                                                                                                                               | FAIL (pre-existing; reproduced unchanged at `d0fc10afa7` in `/tmp/zzz-baseline.ISi24q`) |
| Prettier for documentation changed in this continuation                                                                                                                                                                             | PASS                                                                                    |
| English / Simplified Chinese locale JSON and key-set comparison                                                                                                                                                                     | PASS                                                                                    |
| `./script/check-philosophy`                                                                                                                                                                                                         | PASS                                                                                    |
| `./script/backfill-upstream-ledger` and `./script/check-upstream-ledger`                                                                                                                                                            | PASS                                                                                    |
| macOS / Windows runtime checks                                                                                                                                                                                                      | NOT RUN                                                                                 |
| `cargo test --workspace`                                                                                                                                                                                                            | NOT RUN                                                                                 |

The general reviewed baseline advances to
`3fec4830142a48c08a0e9b8aa241eeaa1a261370`.

## Final Catch-up Batch

### Scope

- Target branch: continued `sync/upstream-2026-10-04` from
  `7c1601ec8c092bc93b7a49b2c9bc62e0ac1f26ad`
- Previous reviewed baseline: `3fec4830142a48c08a0e9b8aa241eeaa1a261370`
- Reviewed head: `279fe070bb389b79652e52065b2f001edcc0b11b`
- Live upstream head at selection and final refresh:
  `279fe070bb389b79652e52065b2f001edcc0b11b`
- Final query time: `2026-10-05T04:46:26Z`
- Reviewed range: all 8 commits after `3fec483014`, oldest first
- Remaining after this batch: 0 commits

The previous baseline and reviewed head are both ancestors of the final
`FETCH_HEAD`.

### Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                                                |
| ---------- | ----- | ------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| badfb8d31f | A     | 15df077f8e   | Clean absorption: single-sided documentation keybinding pills now fall back to their only value instead of rendering empty on Linux.                                                                                       |
| 80ee0a3563 | B     | a55a95bfad   | Added a localized Show Source action to active Markdown preview tabs and hid Markdown/SVG preview actions on inactive editor tabs.                                                                                         |
| d9afb21688 | A     | cf4f21f3fd   | Clean absorption: added configurable self-hosted Gerrit remote parsing and commit/file permalink generation with focused tests.                                                                                            |
| 01376d4926 | A     | d4e991d66b   | Clean absorption: added Gitiles providers, path-prefix-preserving links, settings schema, tests, and documentation.                                                                                                        |
| a9c0144d00 | B     | 57b36b1d57   | Moved branch and tag chips into ZZZ's split Git Graph gutter, added overflow ref menus and dashed connectors, and retained localized custom-command handling.                                                              |
| 5eed2397a8 | B     | 4624cc58c5   | Added localized lightweight-tag creation at repository HEAD for local and remote projects, rejected overwrites, refreshed tag consumers, and documented that tags are not pushed automatically.                            |
| a84689073d | C     | --           | Reimplements `PathStyle` around the absent `crates/path` component and Windows-prefix architecture; ZZZ uses `util::paths` and deliberately gates foreign-style repository identity paths, so the rewrite is not isolated. |
| 279fe070bb | A     | db85220999   | Clean absorption: removed JSON language-server quotes from completion `filterText` so settings keys rank by their actual typed prefix.                                                                                     |

Totals: four `A`, three `B`, one `C`.

### Applied Work

#### Documentation keybinding fallback

`15df077f8e` keeps hand-written single-value keybinding pills visible on Linux
while preserving the existing macOS/Linux split behavior for generated pairs.

#### Markdown preview source action

`a55a95bfad` exposes the existing `CloseAndReturnToEditor` action as a localized
Show Source item on the active preview tab. Preview actions are now withheld for
inactive tabs because those items are outside the dispatch tree. The port reuses
the existing editor preview locale keys and adds the new Show Source key to both
catalogs.

#### Gerrit and Gitiles providers

`cf4f21f3fd` adds the focused Gerrit provider, including authenticated-path
normalization and single-line Gitiles anchors. `d4e991d66b` adds the more general
Gitiles provider, preserves configured URL path prefixes, exposes it through the
settings schema, and documents the browse-root requirement.

#### Git Graph ref gutter

`57b36b1d57` ports the visible Git Graph redesign onto ZZZ's separately split
`git_graph` crate. Ref chips now precede their commit nodes, multiple refs collapse
behind a `+N` badge with per-ref actions, dashed connectors link badges to nodes,
and ringed nodes preserve selected and hovered row backgrounds. ZZZ keeps its
localized menu labels and existing custom Git command task resolution. The
upstream file layout and unavailable prior column-visibility menu infrastructure
were omitted.

#### Create tag at HEAD

`4624cc58c5` adds `git::CreateTagAtHead`, a localized single-line modal, guarded
`git update-ref` creation, remote protocol support, success/error notifications,
and refresh events for Git Graph and Git-panel history consumers. The action
creates lightweight local tags and never pushes them automatically.

#### JSON completion ranking

`db85220999` decodes quoted JSON `filterText` values before fuzzy ranking without
changing the inserted completion text. Invalid or unquoted filter strings remain
untouched.

The four direct `A` changes used `git cherry-pick -x -s`. The three `B` commits
include `Upstream`, `Retained`, and `Omitted` trailers.

### Rejected Work

- `a84689073d` requires upstream's absent `crates/path`, its copied standard-library
  component parser, Windows prefix parser, and earlier `PathStyle::file_name` /
  `parent` API. ZZZ's corresponding code remains in `util::paths`, and its current
  repository identity path intentionally falls back to local-style `std::path`
  handling. Importing the rewrite would be a cross-crate path architecture
  migration rather than an isolated bug fix.

### Verification

| Check                                                                                                                                                                         | Result  |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------- |
| Final live `git ls-remote` and `git fetch --no-tags` to `FETCH_HEAD`                                                                                                          | PASS    |
| Previous baseline and reviewed head are ancestors of final `FETCH_HEAD`; remaining commit count is zero                                                                       | PASS    |
| `git diff --check` and `cargo fmt --all -- --check`                                                                                                                           | PASS    |
| `cargo check --locked` for `editor`, `markdown_preview`, `git_hosting_providers`, `settings_content`, `git_graph`, `git`, `fs`, `project`, `proto`, `git_ui`, and `languages` | PASS    |
| Markdown preview active/inactive tab-menu regressions                                                                                                                         | PASS    |
| `git_hosting_providers` unit and doc tests (138 unit tests)                                                                                                                   | PASS    |
| Git Graph ref layout, custom command, navigation, serialization, stash refresh, and tag refresh suite (26 unit tests)                                                         | PASS    |
| Non-overwriting ref creation, create-tag modal, and JSON completion filter regressions                                                                                        | PASS    |
| Documentation preprocessor unit tests (9 tests) and Prettier checks for `theme/plugins.js` and `src/git.md`                                                                   | PASS    |
| English / Simplified Chinese locale JSON and 3,555-key comparison                                                                                                             | PASS    |
| `./script/check-philosophy`                                                                                                                                                   | PASS    |
| `./script/backfill-upstream-ledger` and `./script/check-upstream-ledger`                                                                                                      | PASS    |
| macOS / Windows runtime checks                                                                                                                                                | NOT RUN |
| `cargo test --workspace`                                                                                                                                                      | NOT RUN |

The general reviewed baseline advances to
`279fe070bb389b79652e52065b2f001edcc0b11b`.
