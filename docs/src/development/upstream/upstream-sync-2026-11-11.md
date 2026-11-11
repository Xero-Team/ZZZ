---
title: Upstream Sync 2026-11-11
description: Selective Zed upstream sync, the first 20 commits after 2026-11-10 batch B.
---

# Upstream Sync 2026-11-11

## Scope

- Local base: `main` at `76f3d4fa1650ffacaebed199f21795ad86be993c`
- Target branch: `sync/upstream-2026-11-11`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `f0d896f2c18a543335eb155f729d36b624695591`
- Live upstream head queried: `43bd5d3cacadd6087c237fe2843f260595b933a7`
- Query time: `2026-11-11`
- Reviewed range: `f0d896f2c1..20f87a421d`, the first 20 of 25 commits, oldest first
- New reviewed baseline: `20f87a421daafc618242acb267e88928da2ef72b`
- Remaining after this batch: `5255d9c721`, `5014e5b302`, `fdb60b8003`,
  `64ef8026c7`, `43bd5d3cac`

The range from the 2026-11-10 live head to the 2026-11-11 live head is 25
commits. This report covers the default batch of the first 20 and stops before
`5255d9c721`, so the next run starts after `20f87a421d`.

ZZZ keeps upstream's `crates/zed/` files under `crates/zzz/`, so
`crates/zed/src/zed.rs` is read as `crates/zzz/src/zzz.rs` and
`crates/zed/src/zed/open_listener.rs` as `crates/zzz/src/zzz/open_listener.rs`.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                                                                                                                                                                   |
| ---------- | ----- | ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 58e29b637c | A     | 0b6f9dd195   | Stops document color inlays from multiplying when switching render modes. Cherry-picked intact; `BufferColors::inlay_colors` was still the pre-image and no hunk touched a diverged region.                                                                                                                                                   |
| bfcd5949ff | C     | --           | Removes the Wayland frame-callback backdating in favour of receipt time. ZZZ's Wayland backend has no frame-timing capture at all -- `Dispatch<WlCallback>` calls `window.frame()` directly -- and the commit that introduced the timing was rejected as needing an absent upstream crate (`4f6d97b9de`), so there is nothing here to remove. |
| 0929d569fa | C     | --           | Reports the Linux desktop environment and display server in telemetry. ZZZ has no `crates/client/src/telemetry.rs` and no `crates/telemetry_events`; telemetry is absent by policy.                                                                                                                                                           |
| 8b31ad99b8 | C     | --           | Adjusts the triage-queue board automation. Upstream triage and board workflow infra has no ZZZ counterpart.                                                                                                                                                                                                                                   |
| a1c03cf6cd | C     | --           | Surfaces Anthropic input transformations in completion events. The `crates/anthropic` crate is absent locally; commercial model providers stay silent manual configuration.                                                                                                                                                                   |
| b5be8e79f7 | C     | --           | Keeps an empty plan from hiding the visible plan. The fix edits `visible_plan`, `PlanIdentity`, and `upsert_plan_items`, which belong to the multi-plan model that is absent locally; local `AcpThread` still carries a single `plan: Plan` with `update_plan`, so this requires an unabsorbed prerequisite.                                  |
| 1b93c47b17 | C     | --           | Replaces the positional `match_strings` arguments with `StringMatchOptions`/`PathMatchOptions` across 16 files. A behaviour-neutral API-shape refactor that needs absent `MaxResults` and `is_cancelled` helpers; no local bug or user-visible behaviour to gain.                                                                             |
| b1962f7cd8 | C     | --           | Updates the Jupyter dependencies and adapts `crates/repl` to the new jupyter-protocol and runtimelib types. Lockfile and dependency churn with no independent ZZZ behaviour.                                                                                                                                                                  |
| 3def8bad0b | C     | --           | Adds protected file names to the agent sandbox. ZZZ has no `crates/sandbox`; the native agent sandbox surface is rejected.                                                                                                                                                                                                                    |
| 0d1756eb10 | B     | 02baa311cf   | Normalises the branch name when renaming. `normalize_branch_name` moved into the `git` crate and the rename modal now normalises on confirm and previews the result.                                                                                                                                                                          |
| 415d310e52 | B     | 4815a24aa9   | Avoids copying and comparing the bytes of shared text. `SharedString` compares pointers first and a `&'static str` child uses `new_static`; the `TextLayout::layout` call keeps ZZZ's extra argument, so the hunk was ported by hand.                                                                                                         |
| f8cfc7ed9e | A     | e0464a2db9   | Fixes duplicated extension snippets after a reload. Cherry-picked intact; `SnippetRegistry` was still keyed by `SnippetKind` alone.                                                                                                                                                                                                           |
| 985bbf42e6 | A     | 0d493ad751   | Applies glyph mark offsets in CosmicTextSystem layout. Cherry-picked intact; `ShapedGlyph` was still built from `glyph.x`/`glyph.y` only.                                                                                                                                                                                                     |
| 912159c7ad | B     | dbcad809f5   | Disambiguates repository names in the repository selector and the panel footer. `GitStore::display_names` and `disambiguated_display_names` landed, and the selector and footer consume them; the selector's local helper functions were kept.                                                                                                |
| 92cc693f0e | C     | --           | Documents hosted Claude pricing. ZZZ has no `docs/src/account/`; hosted-model docs are rejected.                                                                                                                                                                                                                                              |
| cffa8d94a8 | B     | 770bd7a166   | Stops which-key from dismissing an open modal. ZZZ's slimmed `which_key.rs` takes the single-line `has_active_modal` check; the upstream workspace-level test harness was omitted as test-only.                                                                                                                                               |
| 82f2558e17 | C     | --           | Streams the directory check instead of joining all futures. A behaviour-neutral refactor that reshapes three public path APIs ZZZ rewrote in the previous batch and returns the same boolean, so there is no behaviour to gain.                                                                                                               |
| cd4902bfa2 | C     | --           | Bumps Zed to v1.30.0. Upstream release metadata; ZZZ tracks its own version.                                                                                                                                                                                                                                                                  |
| a19e19440d | C     | --           | Adds a backup Ubuntu package mirror to CI. Upstream CI infra.                                                                                                                                                                                                                                                                                 |
| 20f87a421d | C     | --           | Rejects unknown fields in the agent grep tool input. ZZZ's `crates/agent/src/tools/` carries only `context_server_registry.rs`; the grep tool is absent locally.                                                                                                                                                                              |

Totals: three `A`, four `B`, thirteen `C` across 20 commits.

## Applied Work

- `0b6f9dd195` is the intact cherry-pick of `58e29b637c` (`git cherry-pick -x -s`).
- `e0464a2db9` is the intact cherry-pick of `f8cfc7ed9e` (`git cherry-pick -x -s`).
- `0d493ad751` is the intact cherry-pick of `985bbf42e6` (`git cherry-pick -x -s`).
- `02baa311cf` is a `B` port of `0d1756eb10`.
- `4815a24aa9` is a `B` port of `415d310e52`.
- `dbcad809f5` is a `B` port of `912159c7ad`.
- `770bd7a166` is a `B` port of `cffa8d94a8`.
- `576a21f8a1` fixes the ports themselves: the disambiguation test used
  upstream's `PathStyle::Unix`, which ZZZ names `Posix`, and rustfmt had not
  been applied to `branch_picker.rs` and `repository_selector.rs`.

Every `B` commit carries `Upstream`, `Retained`, and `Omitted` trailers and is
signed off for DCO. No remote, pull request, or named upstream remote was
created.

## Per-Commit Narrative

### bfcd5949ff — Wayland frame callback timing (C)

Upstream removed `frame_callback_signal_at` and `frame_callback_instant` and
replaced them with `PlatformFrameSignal::capture(Instant::now)`. ZZZ's
`crates/gpui_linux/src/linux/wayland/client.rs` has neither helper:
`Dispatch<WlCallback>` calls `window.frame()` with no timing at all, and
`PlatformFrameSignal` does not appear anywhere in the workspace. The frame
timing arrived upstream through the per-window refresh-rate journaling that was
rejected as telemetry in `4f6d97b9de`, so this follow-up has no local anchor and
nothing to remove.

### b5be8e79f7 — empty plans must not hide the visible plan (C)

The fix is small but sits on top of #66345: it rewrites `plan_removed` into
`fall_back_from_visible_plan`, consults `plan.is_empty(cx)` in
`replace_plan`, and keeps `visible_plan` across empty `upsert_plan_items`
updates. Local `AcpThread` stores one `plan: Plan` and updates it through
`update_plan`/`snapshot_completed_plan`; there is no `plans` map, no
`PlanIdentity`, and no `visible_plan`. Porting the visible-plan rule would mean
absorbing the multi-plan model first, so the commit stays rejected.

### 1b93c47b17 — `StringMatchOptions`/`PathMatchOptions` (C)

Every call site changes shape, from
`match_strings(candidates, query, case, length_penalty, max_results)` to
`match_strings(candidates, query, options)`. Local `fuzzy_nucleo` still exposes
the positional signatures and has neither the `MaxResults` enum nor the
`is_cancelled` helper the new structs are built on. The refactor changes no
behaviour, so it buys ZZZ nothing and would rewrite 16 files for readability
alone.

### 0d1756eb10 — normalise the branch name when renaming (B)

`normalize_branch_name` lived twice: privately in `git_ui/branch_picker.rs` for
the picker, and not at all for `git::RenameBranch`. The port moves the function
into `crates/git/src/git.rs` with the local trim-then-replace behaviour and its
assertions, imports it in the picker, and makes `RenameBranchModal` normalise on
`Confirm`. The modal also subscribes to its editor so the new muted preview
label follows the typing. Folding the picker's private copy into the shared one
is what made the cherry-pick conflict in the first place; the resolution keeps
the picker's doc comment as the function's doc comment.

### 415d310e52 — shared text byte copies (B)

`SharedString` now compares the string pointers and the length before falling
back to `SmolStr::eq`, and `Hash` is written by hand so the hash is unchanged.
A `&'static str` text child uses `SharedString::new_static` instead of copying.
The cherry-pick conflicted only because `state.layout(...)` takes an extra
argument locally; the port keeps the local signature and the upstream
`new_static(self)` deref coercion, which `rustc` accepts for `&mut &'static str`
as the upstream diff assumes.

### 912159c7ad — repository name disambiguation (B)

`GitStore::display_names` zips repository ids with
`disambiguated_display_names`, which labels each repository with its leaf
directory and grows only colliding labels one parent component at a time. The
free function is a straight port; `util::disambiguate`, `PathStyle::separators_ch`
and `PathStyle::primary_separator` already existed locally.

The UI side diverged: `repository_selector.rs` had been refactored into
`sort_by_display_name` and `index_of_active_repository` helpers, and
`git_panel.rs` never had the upstream `GitPanelContextMenu` anchor. The port
therefore threads a `display_names` map through those helpers instead of
inlining the lookups, adds `display_name_of` for the per-row label, and keeps
the `w_full`/`min_w_0`/`truncate` change so long labels fit. `GitStore` also
emits `RepositoryRemoved` for each repository dropped with its last worktree,
which the selector needs in order to refresh its names.

### cffa8d94a8 — which-key must not dismiss a modal (B)

Local `which_key.rs` is a 118-line rewrite of the upstream module and keeps the
same `observe_pending_input` shape, so the behavioural change is one line:
`workspace.active_modal::<WhichKeyModal>(cx).is_some()` becomes
`workspace.has_active_modal(window, cx)`. The upstream commit also carries a
workspace-level test harness with a `TestModal`, `AppState::test`, and theme
setup; ZZZ's module has no such harness and its tests cover only
`FILTERED_KEYSTROKES`, so the tests are omitted as test-only.

### 82f2558e17 — streamed directory check (C)

`visibility_for_paths`, `visibility_for_subpaths`, and
`find_existing_workspace` become `&[impl AsRef<Path>]`, the
`Vec<PathBuf>` conversion disappears, and `join_all(...).any(...)` becomes a
`FuturesUnordered` `.any(...)`. The boolean result is identical, and the
`open_listener` region was rewritten by the previous batch's `597c383bcd` port
(`preferred_workspace_target`). Reshaping three public signatures for a
behaviour-neutral micro-optimisation is churn, not absorption.

## Verification

| Check                                                                                     | Result  | Notes                                                                                                           |
| ----------------------------------------------------------------------------------------- | ------- | --------------------------------------------------------------------------------------------------------------- |
| `cargo check --locked -p editor`                                                          | PASS    | After `0b6f9dd195`.                                                                                             |
| `cargo check --locked -p gpui_shared_string`                                              | PASS    | After `4815a24aa9`.                                                                                             |
| `cargo check --locked -p gpui`                                                            | PASS    | After `4815a24aa9`; the working tree also held unrelated in-progress edits.                                     |
| `cargo check --locked -p git_ui`                                                          | PASS    | After `02baa311cf` and `9a5a32bca0`, which also checks `project`.                                               |
| `cargo check --locked -p which_key`                                                       | PASS    | After `b7a3b4c2a7`.                                                                                             |
| `cargo test --locked -p git --lib test_normalize_branch_name`                             | PASS    | Covers the moved function and the local `" fix leak "` assertion.                                               |
| `cargo test --locked -p project --lib git_store::tests::test_disambiguated_display_names` | PASS    | Covers unique leaves, one-parent and multi-parent collisions, identical paths, Windows separators, empty input. |
| `cargo test --locked -p snippet_provider`                                                 | PASS    | Registry now replaces a file's snippets instead of appending them.                                              |
| `git diff --check`                                                                        | PASS    | No whitespace errors.                                                                                           |
| `cargo fmt --all -- --check`                                                              | PASS    |                                                                                                                 |
| `cargo xtask check upstream-ledger`                                                       | PASS    | Ledger regenerated with `./script/backfill-upstream-ledger`.                                                    |
| macOS and Windows runtime behaviour                                                       | NOT RUN | This host is Linux; the pasteboard and Windows path hunks were rejected or not applicable.                      |
