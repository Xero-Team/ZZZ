---
title: Upstream Sync 2026-11-13
description: Selective Zed upstream sync, the first 20 of 22 commits after the 2026-11-12 batch.
---

# Upstream Sync 2026-11-13

## Scope

- Local base: `main` at `be1bfe60fe`
- Target branch: `sync/upstream-2026-11-13`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `6ceb6fee8738274589b59139df27efc191ec2231`
- Live upstream head queried: `dc93abce2fa6c0bf349d425310db4884aa953467`
- Query time: `2026-11-13`
- Reviewed range: `6ceb6fee87..aff99ecc32`, the first 20 commits, oldest first
- New reviewed baseline: `aff99ecc3249cfa95e720c4d6c1eb4a72105a3ee`
- Remaining after this batch: two commits, `83df5df635` (adds corgi to the Nix
  dev shell) and `dc93abce2f` (keeps macOS notification service calls off the UI
  thread)

The batch is mostly independent fixes; only the GPU and renderer commits carry
the shader and frame-scheduling assumptions that ZZZ replaced. Four commits
landed whole, four were ported onto local APIs, and twelve were rejected, each
for a named missing surface rather than for looking unrelated.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                                                                                                                                                                                                                                                    |
| ---------- | ----- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 78a8526196 | C     | --           | Updates merman to 0.9.3 and reworks label measurement, font caching, theme defaults, and the mermaid render pipeline. The cherry-pick conflicted in eight files, and its `svg_renderer.rs` hunk needs the closure-based emoji fallback selection that local `SvgRenderer` does not have. ZZZ also already pins merman 0.9.3 in `crates/mermaid_render/Cargo.toml`.                                                             |
| eda58e5040 | B     | 88a25b62dd   | Keeps the rerun target when a tasks file is edited without changing the task. Ported onto the local inventory, `Workspace::schedule_task`, and tasks_ui, with the three new tests; payload below.                                                                                                                                                                                                                              |
| 3c846c3d49 | C     | --           | Aligns the sidebar header with a hidden title bar. Needs `platform_title_bar::TitleBarSettings::is_title_bar_visible`, which arrived with the rejected `title_bar.show` setting (`ba06cc5191`), and reworks `crates/sidebar/src/sidebar.rs`, a file that already differs by 6120 lines.                                                                                                                                        |
| 4dff6f8eef | C     | --           | Routes the remaining alacritty emulator access through `crates/terminal/src/alacritty.rs`. ZZZ has no such module: the terminal crate consists of `terminal.rs`, `mappings/`, `pty_info.rs`, and `terminal_hyperlinks.rs`, so this is a behaviour-neutral refactor toward a module boundary that does not exist locally.                                                                                                       |
| 7fab4c0527 | C     | --           | Detects OpenRouter Anthropic model aliases in `crates/language_models/src/provider/open_router.rs`. ZZZ has no `language_models` crate and no `open_router` reference anywhere in the workspace.                                                                                                                                                                                                                               |
| 498af674ab | C     | --           | Yields between filesystem event batches so heavy activity cannot block the background executor. The local watcher is a different architecture (`GlobalWatcher`, `WatcherMode`, `WatchBackend`, per-mode rescan dedup) and dispatches batches from a blocking receive loop, so the upstream `MAX_DISPATCH_BATCH_SIZE` yield inside the flat `FsWatcher` loop has no local anchor.                                               |
| d3fc053d5b | B     | 11a512a5a5   | Sorts scene primitives through a `(key, index)` permutation so only the indices move. Ported with the helper and every sort it replaces; the bench and two unused depth helpers were omitted; payload below.                                                                                                                                                                                                                   |
| 73ad8e3ed0 | B     | 437505c75a   | Strips the URL slash before a Windows drive path once a remote's path style is known. Ported as `url_path_to_path`, `paths_for_candidate`, the `determine_paths_with_positions` hook, and the shared open-listener parser; payload below.                                                                                                                                                                                      |
| 8e474e1598 | A     | --           | Passes premultiplied alpha to the wgpu shaders as a uniform instead of an override constant. Already equivalent: `crates/gpui_wgpu/src/wgpu_renderer.rs` writes `premultiplied_alpha` into `GlobalParams`, both pipelines use `PipelineCompilationOptions::default()`, and `shaders.wgsl` reads `globals.premultiplied_alpha`.                                                                                                 |
| 27bbbc1cc4 | A     | 79d723b66a   | Captures `preproc_call` arguments in the C and C++ highlight queries, with a `@preproc` rule and one query test per language. Every code hunk applied intact; only the dependency manifests conflicted, and both were resolved to keep the local `itertools 0.15.0` pin while adding the `indoc` dev-dependency.                                                                                                               |
| 42fa909546 | C     | --           | Adds NixOS build support to `corgi.toml`. ZZZ has no `corgi.toml`; upstream release and packaging automation is out of scope.                                                                                                                                                                                                                                                                                                  |
| 48741fd17d | B     | 432955772e   | Draws the which-key popup above open modals in its own `MultiWorkspace` slot. Ported with the new slot, the deferred overlay, and the `render_bare` removal; the upstream test harness was omitted; payload below.                                                                                                                                                                                                             |
| 5a40bc8bd7 | C     | --           | Fixes safe-area insets under a strict style CSP in `crates/gpui_web/src/viewport.rs`. That file does not exist locally and `crates/gpui_web` has no safe-area or `env(safe-area-inset-*)` handling at all, so there is nothing to fix.                                                                                                                                                                                         |
| 4fd48c02ac | C     | --           | Fixes Rust 1.100 warnings across 81 files. The cherry-pick conflicted in 39 paths, many of them files and crates ZZZ deleted (`collab`, `sandbox`, `livekit_client`, `open_ai`, `codestral`, `zeta_prompt`, `edit_prediction_*`, `agent/src/thread.rs`, `gpui/src/profiler/journal.rs`); the surviving files already carry the equivalent form from the local 1.100 migration (`103e2ae596`) and the clippy promotion batches. |
| af7a1adb50 | C     | --           | Uses the full context window for Copilot models without the long-context surcharge. ZZZ has no `crates/copilot_chat`; Copilot auth and provider routes are absent by policy.                                                                                                                                                                                                                                                   |
| cb48e27ceb | A     | 815f2167eb   | Prefers `linux-{arch}-musl` ACP registry targets on musl hosts. Landed with `util::libc_type`, the ordered `current_platform_keys`, the rust-analyzer probe removal, and the archive selection; the two conflicted hunks were import blocks resolved to the local module layout.                                                                                                                                               |
| faf97b0a0c | C     | --           | Preserves structured model refusal details through `ChatCompletionEvent`. Needs `crates/language_model_core/src/chat_completion.rs`, `crates/anthropic`, `crates/open_ai`, `crates/google_ai`, and `crates/language_models`, none of which exist locally.                                                                                                                                                                      |
| 69239eb540 | C     | --           | Adds an `icon_only` constructor and `aria_label` to the toggle button. The change is safe, but nothing in ZZZ calls it: upstream consumers arrive in later commits, and a new public component API with no local caller is out of scope.                                                                                                                                                                                       |
| 563319c3f4 | A     | 068feeea0c   | Adds Neovim's `g r t`, `] n`, and `[ n` defaults to `assets/keymaps/vim.json`. Applied intact; `vim::SelectNextSyntaxNode` and `vim::SelectPreviousSyntaxNode` already exist in `crates/vim/src/visual.rs`.                                                                                                                                                                                                                    |
| aff99ecc32 | C     | --           | Adds `TimedTransformation` and animates SVG sprites without rebuilding the scene. The scene side is isolatable, but the render side needs `Window`'s `needs_present`, `next_frame_callbacks`, and a scene animation interval to keep presenting, and local frame state lives in `Window::frame_scheduler`; `crates/gpui/src/window.rs` has none of those fields.                                                               |

Totals: four `A` (one of them already equivalent), four `B`, twelve `C`.

## Applied Work

- `79d723b66a`, `068feeea0c`, and `815f2167eb` are direct `A` commits
  (`git cherry-pick -x -s`).
- `8e474e1598` is an already-equivalent `A`; no code changed.
- `88a25b62dd`, `11a512a5a5`, `437505c75a`, and `432955772e` are `B` ports,
  signed off for DCO.
- `66d14463aa` formats the ported `remote_connections.rs` imports, which
  rustfmt rewrote after the port.

No remote, pull request, or named upstream remote was created.

## Per-Commit Narrative

### 78a8526196 — merman 0.9.3 (C)

The upgrade is a coupled change: `SvgRenderer::measure_text_width` and the font
file cache are added to `crates/gpui/src/svg_renderer.rs`, and
`crates/mermaid_render` adapts label layout, theme variables, and the
`<defs>`/`<use>` label structure to it. A dry-run cherry-pick conflicted in
`Cargo.toml`, `Cargo.lock`, `svg_renderer.rs`, `markdown/src/mermaid.rs`, and
four files under `crates/mermaid_render`, and the `svg_renderer.rs` hunk expects
a `let mut select = |fonts| ...` fallback closure inside the font resolver that
the local renderer does not use.

The dependency itself is not the blocker: `crates/mermaid_render/Cargo.toml`
already pins `merman = "=0.9.3"` with its own feature list, and the workspace
dependency block upstream edits does not exist locally, because ZZZ removed
`livekit` and `libwebrtc` and pinned newer versions of the surrounding entries.
Re-authoring 1376 lines of renderer and query changes is a rewrite, not a port.

### eda58e5040 — rerun target across tasks file edits (B)

Upstream adds `last_scheduled_task_id` to the inventory, filters
`last_scheduled_task` by it, keeps or advances it in `delete_previously_used`,
and replaces both ad-hoc history retentions in `update_file_based_tasks` with
`retain_unchanged_tasks`. It also adds `Workspace::can_schedule_tasks` so the
reevaluated rerun can bail out before resolving, and `apply_rerun_overrides` to
replace duplicated override code.

Local adaptations:

- The `Worktree` arm keeps the `ref location` binding because the local
  `SettingsLocation` is not `Copy` (`vfs_path: Option<vfs::VfsPath>`), so
  binding it by value would move it before the validation error path reads
  `&location`.
- The integration test constructs `SettingsLocation` with the local
  `vfs_path: None` field.
- `crates/tasks_ui/src/tasks_ui.rs` keeps its own imports; upstream's `tree_sitter`
  import belongs to a query path ZZZ does not carry.
- `crates/workspace/src/tasks.rs` adds `App` to the existing import list and
  leaves the localized `log::warn!` disconnect message untouched.

### d3fc053d5b — scene primitive sorting (B)

`Scene::finish` now sorts shadows, quads, underlines, and the three sprite lists
through `sort_in_place`, which sorts `(key, index)` pairs and then moves each
primitive once. `paths` and `surfaces` keep `sort_by_key`, as upstream does.

Omitted: `crates/gpui_platform/benches/primitives.rs` (ZZZ has no
`gpui_platform/benches` directory), the `finish()` `layer_stack` debug assertion
that upstream's pre-image already carried but local `finish()` does not assert,
and `quad_depth_scale`/`quad_depth`, which the local scene has no caller for.

### 73ad8e3ed0 — Windows drive paths over SSH (B)

The retained behaviour is that a decoded URL path is converted for the host's
path style once the style is known: `util::paths::url_path_to_path` performs the
conversion, `determine_paths_with_positions` applies it after connecting,
`find_existing_workspace` converts its own copy per candidate host, and
`OpenRequest::parse_local_file_path` now shares the helper instead of its
`#[cfg(target_os = "windows")]` prefix check.

Local adaptations:

- `PathStyle::Unix` is `PathStyle::Posix` locally, and `url_path_to_path` calls
  the local `is_absolute(&str)` signature.
- `paths_for_candidate` returns `Vec<PathBuf>` because the local
  `Project::visibility_for_paths` and `visibility_for_subpaths` take
  `&[PathBuf]` rather than a generic `&[impl AsRef<Path>]`.
- The `WorkspaceMatching::MatchSubpaths` arm was dropped: the local enum has
  only `None`, `MatchExact`, and `MatchSubdirectory`.

Omitted: the Windows-only tests and the scaffolding they need
(`init_remote_server`, message interceptors, `report_connection_options`), none
of which exist locally; only the mock's `set_path_style` and the
path-conversion test were ported, so the mock also keeps its original
`connection_options`.

### 48741fd17d — which-key above open modals (B)

`MultiWorkspace` gained a `pending_bindings_popup` slot rendered as a deferred
overlay above the sidebar overlay, with `set_pending_bindings_popup` skipping
`notify` when there is nothing to clear. Which-key now observes `MultiWorkspace`,
creates the popup with `cx.new`, subscribes to its `DismissEvent`, and owns it
through that slot. `WhichKeyModal` is no longer a `ModalView`, so its `dismiss`
method and the modal layer's `render_bare` escape hatch are both gone.

Omitted: the upstream `which_key.rs` test harness (`TestModal`,
`TestChordAction`, `store_bindings`, and the three `MultiWorkspace`-level popup
tests), because local `which_key.rs` keeps only the filtered-keystroke list and
carries no such helpers; and the `rounded_corners` argument, because the local
`client_side_decorations` takes four arguments.

### cb48e27ceb — musl ACP registry targets (A)

`util::libc_type::host_libc_type` caches the host libc for the process lifetime,
`current_platform_keys` returns an ordered list of registry keys, and
`platform_keys` prefers `linux-{arch}-musl` with the glibc key as fallback. The
two conflicted hunks were import blocks: `crates/util/src/util.rs` keeps the
local flat module list and gained `#[cfg(target_os = "linux")] pub mod libc_type;`,
and `crates/project/src/agent_server_store.rs` keeps the local
`http_client::github::AssetKind` path and gained `itertools::Itertools`, so the
absent `github_releases` crate was not imported.

### aff99ecc32 — timed SVG animations (C)

The scene half of the commit is small and self-contained: one `u32` padding slot
becomes `SpriteAnimationId`, animations are tracked in `Scene`, and
`advance_animations` moves sprites between frames. The rendering half is not
portable: the window keeps presenting while
`Scene::animation_frame_interval()` is `Some`, which it computes from
`needs_present`, `next_frame_callbacks`, the rendered frame, and an app animation
clock. Local `Window` has none of those fields — its frame state lives in
`Window::frame_scheduler` — so the animation would never advance.

## Verification

| Check                                                                                                                 | Result  | Notes                                                                        |
| --------------------------------------------------------------------------------------------------------------------- | ------- | ---------------------------------------------------------------------------- |
| `cargo test --locked -p project task_inventory`                                                                       | PASS    | 7 passed, including the three tests this batch adds.                         |
| `cargo test --locked -p tasks_ui rerun`                                                                               | PASS    | 3 passed, including `test_reevaluated_rerun_overrides_keep_task_definition`. |
| `cargo test --locked -p languages preproc_call_arguments`                                                             | PASS    | 2 passed, one per language.                                                  |
| `cargo test --locked -p util test_url_path_to_path`                                                                   | PASS    | 1 passed.                                                                    |
| `cargo test --locked -p util test_parse_ldd_version`                                                                  | PASS    | 1 passed.                                                                    |
| `cargo test --locked -p project test_platform_keys`                                                                   | PASS    | 1 passed.                                                                    |
| `cargo test --locked -p recent_projects test_determine_paths_with_positions_converts_url_paths_for_remote_path_style` | PASS    | 1 passed.                                                                    |
| `cargo test --locked -p which_key`                                                                                    | PASS    | 7 passed.                                                                    |
| `cargo check --locked -p gpui`                                                                                        | PASS    | Scene sorting port.                                                          |
| `cargo check --locked -p zzz --features test-support`                                                                 | PASS    | Open-listener and workspace ports.                                           |
| `cargo check --locked --workspace`                                                                                    | PASS    | Whole workspace.                                                             |
| `cargo fmt --all -- --check`                                                                                          | PASS    | After `66d14463aa`.                                                          |
| `git diff --check`                                                                                                    | PASS    |                                                                              |
| `cargo xtask check upstream-ledger`                                                                                   | PASS    | Ledger regenerated from this report.                                         |
| macOS, Windows, and WebGPU runtime behaviour of the rejected graphics work                                            | NOT RUN | This host is Linux and none of that code was absorbed.                       |

Note on per-crate checks: `cargo check -p <crate>` fails for crates that reach
`remote` without `test-support`, because `crates/remote/src/remote_client.rs`
imports `transport::mock`, which is feature-gated. The same failure exists on
`main` at `be1bfe60fe`; the commands above therefore select a sibling package
(`recent_projects`) that enables `remote/test-support`, or the `zzz` app with
`--features test-support`.
