---
title: Upstream Sync 2026-11-08
description: Selective Zed upstream sync, first 20 commits after the 2026-11-04 baseline.
---

# Upstream Sync 2026-11-08

## Scope

- Local base: `main` at `ced90e0c517bf368238225c4363614b7c2cb1376`
- Target branch: `sync/upstream-2026-11-08`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `c7bea343838f42b4c14151393e41f0bbe033c95e`
- Live upstream head queried: `d2a4fb6ded7c470be25d1541281da3882495b65b`
- Query time: `2026-11-09`
- Reviewed range: `c7bea343838f..db43f05e3980`, 20 commits, oldest first
- Remaining after this batch: 7 commits; the next sync starts after
  `db43f05e3980`

The previous baseline is an ancestor of the live head. The full range is
27 commits; this run reviews the first 20 per the batch default. The remaining
seven are, oldest first, `4c90ed88c3`, `e231594b6b`, `1253dde6ec`,
`9a8ab54db4`, `b5cd310b3b`, `94f3eb1b72`, and `d2a4fb6ded`.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| ---------- | ----- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 4814ef66aa | A     | a658ea57da   | Clean cherry-pick. Documents that `preferred_line_length` also controls `editor::Rewrap`, in default settings, the settings struct doc, and the settings catalog.                                                                                                                                                                                                                                                                                                              |
| 2dcdc4f216 | A     | 0312448f55   | Clean cherry-pick. Rebinds JetBrains `ctrl-k`/`cmd-k` to `git_panel::FocusEditor` and registers the action so the shortcut focuses the commit editor.                                                                                                                                                                                                                                                                                                                          |
| d01f125837 | B     | bf0dca5c46   | Fixes mirrored backspace/delete glyphs in native macOS menus. Ported onto ZZZ's `key_to_native`, keeping the existing name and NSEvent constants; runtime NOT RUN (Linux host).                                                                                                                                                                                                                                                                                                |
| 4ba7d82f68 | C     | --           | Rebinds macOS JetBrains `cmd-o` by moving `workspace::Open` into a Workspace context block and unbinding it in the JetBrains layer. ZZZ's JetBrains keymap already binds `cmd-o` to `project_symbols::Toggle` and its default-macos context layout has diverged; the native-menu key-equivalent behavior is macOS-only and untestable here, so the isolation test fails.                                                                                                       |
| 635026fabc | B     | 8e09aca5f3   | Decodes animated WebP frames in `Image::to_image_data` instead of only the first. Kept ZZZ's local `frames_for_image`/GIF shape and added the animated-WebP branch in place, with the upstream regression test retained.                                                                                                                                                                                                                                                       |
| dcf3f98c9a | C     | --           | Adds keepalive status to the Zed-hosted model completion protocol in `crates/language_models_cloud` and `crates/cloud_llm_client`. ZZZ has neither crate; this is cloud/subscription infrastructure with no local caller, so it is rejected on philosophy and absent-architecture grounds.                                                                                                                                                                                     |
| 654efae441 | C     | --           | Rewrites `on_main_thread` and its two WIT callers to use `AsyncFnOnce` instead of boxed futures. ZZZ's local signatures and `since_v0_*` calls differ, and the change is a refactor with no independent ZZZ behavior; unisolatable from the diverged extension host.                                                                                                                                                                                                           |
| ddc9eda925 | C     | --           | Adds a git-panel branch mode (3834 lines) across `branch_diff.rs`, `diff_multibuffer.rs`, `git_panel/branch_mode.rs`, and `git_store/diff_buffer_list.rs`. All four files are absent locally, and the feature builds on the rejected `ContextMenuSubmenu` rewrite; unisolatable from absent architecture.                                                                                                                                                                      |
| 4a6121bb0f | C     | --           | Rewrites Windows COLR emoji compositing from the GPU shader to the CPU (1484 lines) across `direct_write.rs`, `build.rs`, `directx_renderer.rs`, and `platform.rs`. ZZZ's Windows renderer and shader wiring have diverged, and the host is Linux so the Windows target cannot be validated; unisolatable rewrite.                                                                                                                                                             |
| 0ad2e6b976 | A     | 048f613e89   | Clean cherry-pick. Adds bzip2 and bzip2-tar download support to the extension API and host; `extension_host` compiles with the new WIT variants.                                                                                                                                                                                                                                                                                                                               |
| 4304dec732 | C     | --           | Switches the agent-response copy button to `CopyButton` with feedback, scoped to the current turn. ZZZ's `thread_view` uses a different `render_thread_controls` shape, and upstream's test depends on `acp_v2::ContentBlock` and `visible_content` that are absent locally; unisolatable from the ACP v2 surface.                                                                                                                                                             |
| 1478fb5e60 | C     | --           | Implements ACP plan operations in `crates/acp_thread`, `crates/agent_servers/src/acp/v1.rs`, and the agent UI. ZZZ has no `agent_servers/src/acp/` directory and no plan-content machinery; the feature depends on unabsorbed ACP v2 APIs, so it is C on absent-architecture grounds.                                                                                                                                                                                          |
| e40e600719 | B     | baa6598f47   | Makes the commit-message view preserve hard line breaks. Ported as a single `MarkdownStyle` override on ZZZ's `hover_markdown_style` call; the upstream import change is retained.                                                                                                                                                                                                                                                                                             |
| 416cd84963 | A     | 3fb9a92ca1   | Clean cherry-pick. Adds `cx.notify()` after the async load/reload updates in the GPUI `testing` example so the count refreshes immediately.                                                                                                                                                                                                                                                                                                                                    |
| 145bcfc1bc | C     | --           | Fixes terminal links for absolute paths when the remote OS differs, by threading `project.path_style` through `project_paths_to_check` and `normalize_absolute_candidate`. ZZZ's `path_link.rs` uses a completely different resolution path with neither helper nor `BackgroundPathChecks`; the bug's local equivalent does not exist, so the port has no anchor.                                                                                                              |
| 74495e740e | C     | --           | Keeps submenus open while the pointer rests on their trigger during keypress, via `HoverListenerMode::InputModalityIndependent`. ZZZ has no `HoverListenerMode` and its `context_menu.rs` still uses the plain `Submenu` variant; the change depends on the rejected `ContextMenuSubmenu` rewrite, so it is C.                                                                                                                                                                 |
| 7bb5656e63 | C     | --           | Implements `Along` for `Edges` and relaxes the public `Along::apply_along` trait method from `FnOnce` to `Fn`. No local caller uses `Along` on `Edges` (all local `.along()`/`apply_along()` call sites are on `Point`/`Size`), and its only consumer `78fde14d12` is rejected in this batch. A new public API with no current ZZZ caller plus a public signature change; the isolation test fails and the gpui API snapshot would have to be blessed for an unused API, so C. |
| 0018500a6a | C     | --           | Adds crate-owner endpoints to `tooling/crates_io_client`. ZZZ has no `tooling/crates_io_client` member and no publishing caller; upstream-only release infrastructure with no independent ZZZ behavior.                                                                                                                                                                                                                                                                        |
| 78fde14d12 | C     | --           | Fixes ignored horizontal padding on `list()` by insetting item width and origin (279 lines plus five tests). ZZZ's `list.rs` already carries `last_padding`/`max_item_width` with a diverged layout; the core rewrite and its tests are unisolatable, so C.                                                                                                                                                                                                                    |
| db43f05e39 | B     | 1424451599   | Makes bold Markdown text inherit the preview theme color. Ported by removing the editor color override from the `Strong` tag style, matching the upstream patch.                                                                                                                                                                                                                                                                                                               |

Totals: four `A` (all clean cherry-picks), four `B`, twelve `C` across
20 commits.

## Applied Work

- `a658ea57da` is a clean `A` cherry-pick of `4814ef66aa`.
- `0312448f55` is a clean `A` cherry-pick of `2dcdc4f216`.
- `bf0dca5c46` is a `B` port of `d01f125837`.
- `8e09aca5f3` is a `B` port of `635026fabc`.
- `048f613e89` is a clean `A` cherry-pick of `0ad2e6b976`.
- `baa6598f47` is a `B` port of `e40e600719`.
- `3fb9a92ca1` is a clean `A` cherry-pick of `416cd84963`.
- `1424451599` is a `B` port of `db43f05e39`.

Every clean `A` used `git cherry-pick -x -s` and keeps the upstream patch
and message intact. Every `B` was committed with `git commit -s` and
`Upstream:` / `Retained:` / `Omitted:` trailers. No remote, pull request,
or named upstream remote was created.

## Per-Commit Narrative

#### d01f125837 — macOS menu key equivalents (B)

Native macOS menus use different character values than `NSEvent` key-down
events: backspace is `0x08` and delete-forward is `0x7f`, while the key-down
constants are `0x7f` and `NSDeleteFunctionKey`. Using the latter mirrored the
backspace glyph. Upstream renamed `key_to_native` to
`key_to_menu_equivalent`, widened the constants to `u32`, and changed the two
arms. ZZZ's `key_to_native` is referenced in exactly one place: building the
native menu item in `platform.rs`. The port keeps the existing name and the
`u16` constants (used elsewhere for key-down parsing, including
`BACKSPACE_KEY` at line 378) and only adds the two AppKit-equivalent
constants and swaps the two match arms. macOS runtime is NOT RUN.

#### 635026fabc — animated WebP in `to_image_data` (B)

`ImageAssetLoader` already decoded animated WebP frames, but
`Image::to_image_data` still called `frames_for_image` (which decodes a
single frame) for `ImageFormat::Webp`. Upstream extracted shared
`decode_gif_image`/`decode_webp_image` helpers into `platform.rs` and
rewired `img.rs`. ZZZ keeps its local `frames_for_image` and inline GIF
decode and has a diverged `img.rs`, so the port adds the animated-WebP
branch directly in `to_image_data`: when `WebPDecoder::has_animation()` it
decodes all frames with RGBA-to-BGRA conversion, otherwise it falls back to
`frames_for_image`. The upstream 2-frame regression test is retained and
passes.

#### e40e600719 — commit message line breaks (B)

The commit view renders the message as Markdown with the hover style, whose
single newlines are soft breaks, so hard-wrapped bodies reflowed. The port
wraps ZZZ's `hover_markdown_style(window, cx)` in a `MarkdownStyle` with
`soft_break_as_hard_break: true` and adds the `MarkdownStyle` import. This is
the exact upstream behavior on a one-line local call site.

#### db43f05e39 — preview theme for bold text (B)

Bold text forced the editor theme's `text` color, which can be unreadable
when the Markdown preview uses a different theme. Upstream removed the
`color` override from the `MarkdownTag::Strong` style refinement. The port
makes the same one-line removal on ZZZ's identical `Strong` arm, so bold
inherits the surrounding preview color.

#### 4ba7d82f68 — macOS JetBrains `cmd-o` (C)

The upstream fix moves `cmd-o` from the base keymap into a
`Workspace`/`use_key_equivalents` block and adds a `cmd-o: null` unbind in a
new leading Workspace block of `macos/jetbrains.json`, so File > Open no
longer shows the shortcut and `cmd-o` reaches `project_symbols::Toggle`.
ZZZ's `macos/jetbrains.json` already binds `cmd-o` to
`project_symbols::Toggle` directly, and ZZZ's `default-macos.json` keeps
`cmd-o: workspace::Open` in a diverged top-level block. There is no upstream
regression test in ZZZ, the behavior is macOS-native-menu only, and the
local keymap shape is different. A partial context move would be an
untestable, incomplete port, so per the isolation test this is C.

#### dcf3f98c9a — cloud keepalives during compaction (C)

Adds a `KeepAlive` completion status and status-message negotiation for
Zed-hosted models. The touched `crates/language_models_cloud` and
`crates/cloud_llm_client` crates are absent locally: ZZZ ships only
provider-agnostic, silently configured model support. This is cloud and
subscription infrastructure with no local caller, so it is rejected on both
philosophy and absent-architecture grounds.

#### 654efae441 — `on_main_thread` future simplification (C)

Replaces `LocalBoxFuture` with `AsyncFnOnce` in `WasmState::on_main_thread`
and its two WIT callers. ZZZ's `wasm_host.rs` has the same boxed-future
signature but its `since_v0_1_0.rs`/`since_v0_8_0.rs` callers are shaped
around different settings code. The change is a pure refactor with no
independent ZZZ behavior, and adapting the closure shape across the
diverged files would be an unisolatable rewrite, so C.

#### ddc9eda925 — git panel branch mode (C)

Adds 3834 lines across `git_ui/src/branch_diff.rs`,
`git_ui/src/diff_multibuffer.rs`, `git_ui/src/git_panel/branch_mode.rs`, and
`project/src/git_store/diff_buffer_list.rs`. All four paths are absent
locally, and the UI depends on the `ContextMenuSubmenu` embedded-view
rewrite already rejected in the 2026-11-04 sync. Cherry-pick reports the
files deleted by us or missing. Unisolatable from absent architecture, so C.

#### 4a6121bb0f — Windows COLR CPU compositing (C)

Rewrites emoji rendering from GPU shader layers to CPU compositing across
`direct_write.rs` (295+/744-), `build.rs`, `directx_renderer.rs`, and
`platform.rs`, deleting `color_text_raster.hlsl`. ZZZ's Windows renderer
still carries that shader and its `compile_shader_for_module` wiring with a
diverged `direct_write.rs`. The host is Linux, so no Windows target or
runtime validation is possible. A 1484-line platform-specific rewrite with
no local test path is C.

#### 4304dec732 — copy feedback for agent responses (C)

Swaps the copy icon for `CopyButton`, adds `agent_message_copy_index`, and
scopes the copied response to the current turn. ZZZ's
`render_thread_controls` uses a different parameter set and localized
tooltips, and the upstream regression test constructs messages via
`acp_v2::ContentBlock` and `MessageContent::new`, which do not exist
locally (`crates/acp_v2` is absent). The retained behavior depends on the
unabsorbed ACP v2 surface, so C.

#### 1478fb5e60 — ACP plan operations (C)

Adds structured/markdown/file plans, `plan_removed`, and v1/v2
`plan_update` handling. Its files `crates/agent_servers/src/acp/v1.rs`,
`session_updates.rs`, and the agent UI plan rendering have no local
equivalents: ZZZ has no `agent_servers/src/acp/` directory (ACP v2 was not
absorbed) and its `Plan` struct still holds a plain `Vec<PlanEntry>`. The
commit depends on unabsorbed ACP v2 APIs, so C.

#### 145bcfc1bc — remote OS path links (C)

Threads `project.path_style(cx)` into `project_paths_to_check` and
`normalize_absolute_candidate` so `/home/x` is treated as absolute on a
Windows host with a WSL project. ZZZ's `path_link.rs` resolves paths
through `possible_open_target_internal` with `fs_paths_to_check` and
`worktree_paths_to_check`; it has neither `project_paths_to_check` nor
`normalize_absolute_candidate` nor `BackgroundPathChecks`. The upstream
helpers do not exist locally, so there is no anchor for a complete, testable
port and the isolation test fails. C.

#### 74495e740e — submenu hover during keypress (C)

Adds `HoverListenerMode::InputModalityIndependent` and moves `on_hover` onto
the submenu trigger container. ZZZ has no `HoverListenerMode` in GPUI and
its `context_menu.rs` still uses the plain `Submenu` variant; the change is
a follow-up to the `ContextMenuSubmenu` rewrite rejected in 2026-11-04.
New public API with no local caller and a dependency on an unabsorbed
commit, so C.

#### 7bb5656e63 — `Along` for `Edges` (C)

Adds `impl Along for Edges<T>` and changes the trait method
`Along::apply_along` from `impl FnOnce(Self::Unit) -> Self::Unit` to
`impl Fn(Self::Unit) -> Self::Unit` so the scrollbar call sites can clone a
value. It was initially absorbed as a clean `A`; the full `cargo xtask
check` then failed the `gpui` public API snapshot, which correctly flagged
the public trait signature change. Re-reviewing the isolation test showed
that no local caller uses `Along` on `Edges` (every `.along()` and
`apply_along()` call site in ZZZ is on `Point` or `Size`), and the only
consumer of the new impl, `78fde14d12`, is rejected in this same batch.
That is a new public API with no current ZZZ caller, and blessing the API
snapshot for an unused signature change would weaken the snapshot gate for
no local behavior. The clean `A` cherry-pick was removed from the work
branch with `git rebase --onto` before any push, and the commit is
classified C.

#### 0018500a6a — crates.io owner endpoints (C)

Adds list/add/remove crate-owner endpoints to `tooling/crates_io_client`.
ZZZ has no `tooling/crates_io_client` member (the whole crate was rejected
in the 2026-11-04 sync) and no extension-publishing caller. Upstream-only
release infrastructure with no independent ZZZ behavior, so C.

#### 78fde14d12 — `list()` horizontal padding (C)

Insets item width by horizontal padding and offsets item origin in
`crates/gpui/src/elements/list.rs`, adding `Edges::Add`, a `content_insets`
helper, and five GPUI tests. It depends on `Along for Edges` (`7bb5656e63`,
rejected in this batch) but is a 279-line rewrite of ZZZ's diverged
`prepaint_items`/`layout_items` path, which already tracks `last_padding`
and `max_item_width` with a different layout. The core rewrite and its tests
are unisolatable from that divergence, so C.

## Verification

| Check                                                                                            | Result  | Notes                                           |
| ------------------------------------------------------------------------------------------------ | ------- | ----------------------------------------------- |
| `cargo check --locked -p settings_content -p git_ui -p gpui -p ui -p markdown -p extension_host` | PASS    | after the four clean `A` cherry-picks           |
| `cargo check --locked -p gpui --examples`                                                        | PASS    | includes `testing.rs` notify fix                |
| `cargo check --locked -p extension_host`                                                         | PASS    | after the bzip2 WIT variants                    |
| `cargo check --locked -p gpui_macos -p gpui -p git_ui -p markdown`                               | PASS    | after the four `B` ports                        |
| `cargo test --locked -p gpui test_animated_webp_image_to_image_data_decodes_frames`              | PASS    | 1 test                                          |
| `cargo xtask check gpui-api`                                                                     | PASS    | `7bb5656e63` removed; snapshot matches again    |
| `cargo fmt --all -- --check`                                                                     | PASS    | clean                                           |
| `git diff --check` on every applied commit                                                       | PASS    | clean                                           |
| `cargo xtask check`                                                                              | PASS    | all 17 repository gates                         |
| macOS and Windows runtime checks                                                                 | NOT RUN | host is Linux; those backends are platform-only |
| `cargo test --workspace`                                                                         | NOT RUN | narrowest per-crate checks were used            |
| Remaining commits through the live head                                                          | 7       | next sync starts after `db43f05e3980`           |

The reviewed baseline advances to
`db43f05e398036f08e544c03cbc068216c7a0aaa`.
