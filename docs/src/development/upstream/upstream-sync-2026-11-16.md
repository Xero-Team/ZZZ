---
title: Upstream Sync 2026-11-16
description: Selective Zed upstream sync, the first 20 of 37 commits after the 2026-11-13 batch.
---

# Upstream Sync 2026-11-16

## Scope

- Local base: `main` at `a444b27f98`
- Target branch: `sync/upstream-2026-11-16`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `aff99ecc3249cfa95e720c4d6c1eb4a72105a3ee`
- Live upstream head queried: `917559da35dd4d626205da705b071c800c9c2553`
- Query time: `2026-11-16`
- Reviewed range: `aff99ecc32..917559da35`, the first 20 commits, oldest first
- New reviewed baseline: `596dfdcf32d85d4d8d17a8887b4d0979ec7c5e14`
- Remaining after this batch: seventeen commits, starting with `24d37721115`
  and `021c7d4c4f`

The batch is dominated by narrow performance and correctness fixes in `gpui`,
`proto`, `worktree`, `fs`, `language`, and the renderer. Three of the four
rejections are missing local surfaces rather than philosophy: the local tree has
no system-notification API in `gpui`, no `open_ai` provider crate, and an ACP SDK
that is three minor versions behind the one the commit ports onto.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                                                                                                                                                                                         |
| ---------- | ----- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 83df5df635 | C     | --           | Adds `corgi` to the Nix dev shell. ZZZ has the `nix/` tree but no `corgi.toml` at the repo root and no `corgi` reference anywhere in `nix/`, so the devshell input has nothing to install. Upstream-only build tooling.                                                                                                                                             |
| dc93abce2f | C     | --           | Moves macOS notification service calls off the UI thread and returns a `Task<()>` from `dismiss_system_notification`. Local `gpui` has no `ShowSystemNotification`/`dismiss_system_notification` pair at all — the whole `Platform` notification surface is absent — so there is nothing to make asynchronous.                                                      |
| 06999050d6 | B     | 6baa56dfa8   | Streams thread saves through a `BufWriter` into the zstd encoder instead of buffering the JSON. The cherry-pick conflicted in the `std` import block only; payload below.                                                                                                                                                                                           |
| 5bd59e2e43 | C     | --           | Adds GPT-6.1 Terra to `crates/open_ai` and `crates/language_models`. ZZZ has neither crate, and `crates/language_model/src` carries only `api_key.rs`, `language_model.rs`, and `request.rs`, so the model catalog has no local home.                                                                                                                               |
| f0fa51bb51 | A     | --           | Gates a `DisplayId` import on `test-support`. Already equivalent: local `crates/gpui/src/platform/test/platform.rs` imports neither `DisplayId` nor the other bench-only names, so `--features bench-support` has no unused import to warn about.                                                                                                                   |
| d0400aaf03 | A     | 2ac56f46e0   | Matches keystrokes before evaluating context predicates. Applied intact.                                                                                                                                                                                                                                                                                            |
| f53c9ae3d6 | B     | 543f8c364b   | Splits worktree updates in linear time by draining both entry lists through iterators. Payload below.                                                                                                                                                                                                                                                               |
| 0d3c40e1b9 | A     | 83c07da501   | Sorts removed entry ids once instead of inserting each at its sorted position. Applied intact.                                                                                                                                                                                                                                                                      |
| 0cfc4a483d | B     | 6afbf5218d   | Moves the `async-task` pin to the revision carrying the waker identity fix. Local already pinned smol-rs, so only the revision changed. Payload below.                                                                                                                                                                                                              |
| 2568d560f1 | B     | c869ba5861   | Corrects setting doc comments that disagree with `assets/settings/default.json`. Payload below.                                                                                                                                                                                                                                                                     |
| 99c890d39d | B     | 9d6b468706   | Folds the watcher path once when matching ancestors on non-Windows platforms. Payload below.                                                                                                                                                                                                                                                                        |
| 4cc3e27d9d | A     | 6e99e0d014   | Builds completion sort keys once per match with `sort_by_cached_key`, with a stability test. Applied intact.                                                                                                                                                                                                                                                        |
| 62cb1ac7b1 | A     | --           | Shapes a line from `line.as_str()` instead of cloning it. Already equivalent: `crates/editor/src/element.rs` already passes `line.as_str().into()`, and no `line.clone().into()` call remains.                                                                                                                                                                      |
| 56a157e5b0 | B     | 456fff179a   | Resets unused-code state when seeking buffer chunks. Payload below.                                                                                                                                                                                                                                                                                                 |
| 15d136bf8e | B     | 17dfae0022   | Stops suggesting the `r` extension for R Markdown and Quarto files. Payload below.                                                                                                                                                                                                                                                                                  |
| 703b5ac3f4 | C     | --           | Updates `agent-client-protocol` to 5.3.0 (schema 1.18.0) and follows its rule changes for plan files, v2 command types, and the `speed` config category. Local `acp_thread` has `PlanEntry` but no `PlanContent`, `config_options.rs` is absent, and `Cargo.toml` pins `=5.0.0`, so every hunk needs the 5.1/5.2 plan and option machinery that was never absorbed. |
| e2f00a41ac | B     | 67391f1d16   | Fixes antialiased outline compositing: `over` guards a zero alpha, and the quad fragment scales the border alpha instead of blending then interpolating. Payload below.                                                                                                                                                                                             |
| fb62376caa | A     | 87625235a1   | Reads the buffer length through `buffer.len()` instead of copying the text. Applied intact.                                                                                                                                                                                                                                                                         |
| 5f27a0a02a | B     | d64fafaedf   | Moves fuzzy segment results instead of cloning them. The cherry-pick conflicted in three of four files because local `fuzzy_nucleo` keeps its own harvest shape. Payload below.                                                                                                                                                                                     |
| 596dfdcf32 | A     | 0b0c86d69b   | Resets `max_start_depth` when a query cursor returns to the pool, with a regression test. Applied intact.                                                                                                                                                                                                                                                           |

Totals: seven `A` (two already equivalent), nine `B`, four `C`.

## Applied Work

- `2ac56f46e0`, `83c07da501`, `6e99e0d014`, `87625235a1`, and `0b0c86d69b` are
  direct `A` commits (`git cherry-pick -x -s`).
- `f0fa51bb51` and `62cb1ac7b1` are already-equivalent `A`; no code changed.
- `6baa56dfa8`, `543f8c364b`, `6afbf5218d`, `c869ba5861`, `9d6b468706`,
  `456fff179a`, `17dfae0022`, `67391f1d16`, and `d64fafaedf` are `B` ports,
  signed off for DCO.

No remote, pull request, or named upstream remote was created.

## Per-Commit Narrative

### 83df5df635 — corgi in the Nix dev shell (C)

The commit bumps `corgi.toml` to `corgi_version = "0.1.106"`, adds a
`corgi-src` flake input, and wires the pinned source into
`nix/modules/devshells.nix`. ZZZ keeps a `nix/` tree, but `corgi.toml` does not
exist at the repo root and `grep -r corgi nix/` finds nothing, so the devshell
addition would reference a config file and a workflow the fork does not have.
The earlier `42fa909546` rejection is the same surface.

### dc93abce2f — macOS notifications off the UI thread (C)

The commit changes `Platform::dismiss_system_notification` to return
`Task<()>`, updates the Linux, Windows, and test platforms, and rewrites
`gpui_macos/src/system_notifications.rs` around a dedicated worker thread. Local
`gpui` carries none of it: `grep -rn system_notification crates/` returns no
matches, and `crates/gpui/src/platform.rs` has no notification methods. This is
the same missing-surface family as `8e474e1598`'s neighbours rather than a
rejection on philosophy grounds.

### 06999050d6 — streaming thread saves (B)

The retained behaviour is that `save_thread_sync` serializes into a `BufWriter`
wrapped around a `zstd::stream::Encoder` and finishes the encoder, so the
uncompressed JSON is never materialized next to the compressed blob. The
compression level (`3`), the flattened `version` field, and the load path are
unchanged.

Local adaptation: the local file has drifted at the top. It imports
`db::ZZZ_STATELESS` instead of `zed_env_vars::ZED_STATELESS` and declares
`DbMessage`/`DbLanguageModel` as `serde_json::Value`, so the upstream import
block does not apply; only `BufWriter` and `Write as _` were added to the
existing `use std::...` statement. The new
`test_save_thread_stores_versioned_json` was ported unchanged.

### 5bd59e2e43 — GPT-6.1 Terra (C)

The change adds a `Model::SixPointOneTerra` variant and wires it through
`is_reasoning_model`, `max_tokens`, `temperature_for_model`, and the provider
switch in `crates/language_models/src/provider/open_ai.rs`. `crates/open_ai` and
`crates/language_models` do not exist locally; `crates/language_model` holds
only the provider-agnostic API surface. Silent manual OpenAI-compatible
providers are in scope, but this catalog entry has no local file to land in.

### f53c9ae3d6 — linear-time worktree update splitting (B)

`split_worktree_update` now takes both entry vectors with `mem::take` and drains
them through `Iterator::take`, replacing the `Vec::drain(..n)` calls that shifted
the remaining entries on every chunk.

Omitted: upstream's `updated_repositories` chunking and the `limit` bookkeeping
that goes with it. Local `UpdateWorktree` has no `updated_repositories` field, so
`done` is computed from the two entry iterators alone, which is the same
condition the local pre-image used. The `cmp` import became unused and was
dropped.

### 0cfc4a483d — async-task pin (B)

Local `[patch.crates-io]` already pointed at `smol-rs/async-task` rather than
the `zed-industries` fork, at `b4486cd7` (upstream's own January bump). The port
moves the revision to `41fd3359`, which carries the waker identity fix tracked by
smol-rs/async-task#108, and keeps the local comment style by adding the upstream
rationale comment above the entry. `Cargo.lock` was regenerated with
`cargo metadata`, not hand-edited.

### 2568d560f1 — stale setting defaults in doc comments (B)

The retained behaviour is documentation only: the `Default:` line for each field
now matches the value in `assets/settings/default.json`. Verified per field
against local defaults: `thinking_display: "auto"`, `diagnostics_max_severity:
"all"`, `show_author_name: true`, `prefill_query_from_selection: false`,
terminal `line_height: "standard"`, terminal `bell: "off"`,
`pane_split_direction_horizontal: "down"`, `pane_split_direction_vertical:
"right"`, and `show_close_button: "hover"`.

Omitted: the `base_keymap` hunk. Upstream now documents `Default: Zed`, but local
`assets/settings/default.json` still ships `"base_keymap": "VSCode"`, so the
existing comment is accurate for this fork and rewriting it would make the
documentation wrong. The `expand_excerpt_lines` hunk was already at `Default: 5`
locally.

### 99c890d39d — folded watcher path matching (B)

`WatchKey::folded_path` now returns the folded spelling as a `String`, with
`WatchKey::folded` delegating to it. `watcher_ids_covering` folds the event path
once on non-Windows platforms and walks the ancestors of that folded path,
instead of constructing a folded `WatchKey` per ancestor; Windows keeps the
per-ancestor lookup because folding there is a no-op.

Omitted: nothing. The local `covered_by_recursive_ancestor` predicate still
probes both key forms, as it did before; upstream did not change it either.

### 56a157e5b0 — unused-code state across chunk seeks (B)

`initialize_diagnostic_endpoints` now clears `unnecessary_depth` with the other
severity depths, so seeking out of an unnecessary-code diagnostic stops leaving
later text marked unnecessary. The regression test
`test_chunks_seek_out_of_unnecessary_diagnostic` was ported.

Local adaptations: upstream also deleted a duplicate
`initialize_diagnostic_endpoints()` call inside `seek`; local `seek` already calls
it once at the end, so there was nothing to delete. The test constructs its entry
as a `DiagnosticEntry` literal because local `DiagnosticEntry` has no `new`
constructor.

### 15d136bf8e — R extension suggestions (B)

The retained behaviour is that `Rmd`, `rmd`, and `qmd` no longer map to the `r`
extension suggestion. Local keeps the table in
`crates/extensions_ui/src/extension_suggest.rs` rather than a separate
`extension_suggest` crate, and no local test referenced the removed suffixes.

### 703b5ac3f4 — ACP SDK 5.3.0 (C)

The commit is a coupled SDK upgrade plus conformance work: `PlanContent::File`
is replaced by an optional `path` on the markdown plan, unknown v2 command types
stop being offered as slash commands, and a new `speed` config category is
accepted by the v1 conversion. Local `crates/acp_thread` is a flat v1-only
module set (`acp_thread.rs`, `connection.rs`, `diff.rs`, `mention.rs`,
`terminal.rs`) with `PlanEntry` but no `PlanContent`, and `Cargo.toml` pins
`agent-client-protocol = "=5.0.0"`. The 5.1 and 5.2 plan and config-option
machinery this builds on was never absorbed, so every hunk lands on an absent
type rather than on a divergent one.

### e2f00a41ac — antialiased outline compositing (B)

Retained: `over` divides by a guarded denominator, and the quad fragment
multiplies `border_color.a` by the inner antialias coverage before a single
`over` pass instead of compositing the border first and interpolating back
against the background. Applied to Metal, WGSL, and HLSL.

Omitted: nothing. The Metal file lives at `crates/gpui_macos/src/shaders.metal`
locally, not upstream's `crates/gpui_apple`.

### 5f27a0a02a — moving fuzzy segment results (B)

Retained: all four harvest sites consume the per-segment vectors with
`into_iter().flatten()` instead of `concat()`, so matches move rather than clone.
Local call sites are `fuzzy/src/{paths,strings}.rs` and
`fuzzy_nucleo/src/{paths,strings}.rs`.

Omitted: upstream's surrounding restructuring. The local `fuzzy_nucleo` harvest
keeps its own shape (`segment_results` zipped with a separate matcher list), and
`fuzzy/src/strings.rs` keeps calling `util::truncate_to_bottom_n_sorted_by`
directly rather than upstream's `rank_matches` helper, which local does not have.

## Verification

| Check                                                                                           | Result  | Notes                                                                         |
| ----------------------------------------------------------------------------------------------- | ------- | ----------------------------------------------------------------------------- |
| `cargo check --locked -p agent -p proto -p worktree -p fs -p extensions_ui -p settings_content` | PASS    | Covers every ported crate and the `editor` dependency chain.                  |
| `cargo check --locked -p language --tests`                                                      | PASS    | Buffer port and its regression test.                                          |
| `cargo check --locked -p fuzzy -p fuzzy_nucleo`                                                 | PASS    | Segment harvest port.                                                         |
| `cargo check --locked --workspace`                                                              | PASS    | Whole workspace; only pre-existing manifest warnings in `vim` and `ztracing`. |
| `cargo test --locked -p agent test_save_thread_stores_versioned_json`                           | PASS    | 1 passed.                                                                     |
| `cargo test --locked -p proto test_split_worktree_update`                                       | PASS    | 1 passed.                                                                     |
| `cargo test --locked -p fs`                                                                     | PASS    | 36 passed, 1 ignored.                                                         |
| `cargo test --locked -p worktree --lib`                                                         | PASS    | 9 passed.                                                                     |
| `cargo test --locked -p worktree --test integration`                                            | PASS    | 104 passed, including the watcher and rescan suites.                          |
| `cargo test --locked -p language test_chunks_seek_out_of_unnecessary_diagnostic`                | PASS    | 1 passed.                                                                     |
| `cargo test --locked -p editor test_sort_keeps_order_of_equal_keys`                             | PASS    | 1 passed.                                                                     |
| `cargo fmt --all -- --check`                                                                    | PASS    | After the ports.                                                              |
| `git diff --check`                                                                              | PASS    |                                                                               |
| `cargo xtask check upstream-ledger`                                                             | PASS    | Ledger regenerated from this report.                                          |
| Metal, HLSL, and Windows/macOS runtime behaviour of the shader change                           | NOT RUN | This host is Linux; the shaders are not compiled or executed here.            |
| `cargo check -p gpui --features bench-support` for `f0fa51bb51`                                 | NOT RUN | Already-equivalent check was made by reading the import list.                 |
