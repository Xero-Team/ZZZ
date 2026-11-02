---
title: Upstream Sync 2026-11-02
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-11-02

## Batch 1 {#batch-1}

### Scope

- Local base: `main` at `e730f1f24f`
- Target branch: `sync/upstream-2026-11-02`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `ba63a022c9a22ca66d68ef2a8f0bb9b5b0a9984d`
- Live upstream head queried: `4ec98df5000b21dca9aa1a064d770fb6ebc63880`
- Query time: `2026-11-02`
- Reviewed range: `ba63a022c9a2..4ec98df5000b`, seventeen commits, oldest
  first
- Remaining after this batch: zero; the range ends at the queried head

The previous baseline is an ancestor of the queried head. The whole
remaining range fits in one batch, so the next sync starts after
`4ec98df5000b`. The primary worktree had uncommitted docs and tooling
work, so the batch ran in a linked worktree branched from `main` at
`e730f1f24f`; that worktree and its uncommitted work were left
untouched.

### Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                                                                                                                            |
| ---------- | ----- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| a4b39c38e0 | B     | 2f6d743bd1   | Adds `rounded_images` to `MarkdownStyle`, defaulting to true, and sets it false in the Markdown preview overrides, so preview images draw square while the agent panel keeps rounded corners. Ported by hand because the local image element is built through a divergent `modify_current_div` path.   |
| d94f32a1d9 | B     | ed2e8d7752   | Rounds the terminal tool header's top corners in card layouts and its bottom corners only when nothing renders below it; the collapsible command no longer paints its own header background. Applied inline in `thread_view.rs`; ZZZ has no `TerminalToolHeader` component.                            |
| c408f8efc4 | B     | db39e5238a   | Returns an `AppBorrowError` from `AppCell::borrow`/`try_borrow_mut` carrying the caller, the borrow kind, and the recorded borrow-group origin, and marks the async and window update helpers `#[track_caller]`. The `window.rs` operation-context hunks were omitted; those sites have diverged.      |
| f816e1f926 | B     | 13f9aaadae   | `MetalRenderer::render_to_image` delegates to `render_scene_to_image`, so macOS screenshots render into a renderer-owned texture instead of taking a drawable the display link paces. The local offscreen path and texture allocation already existed.                                                 |
| 26abd1c959 | C     | --           | Extends the `gpui_platform` benchmarks and adds `script/gpui-bench-compare`, plus benchmark-only `Window::rendered_scene` and `BenchWindowContext::simulate_resize`. ZZZ has no bench targets, no `randomized_element_tree` fixture, and no caller for those APIs.                                     |
| decd2399b1 | B     | b3ad800004   | Records entity reads in a scope stack, so cached views and deferred draws keep subscriptions for the entities they actually read, including when prepaint output is reused. Deferred stores its reads in `frame.rs`; the cache is `AnyView::cached` locally.                                           |
| b41a8b60a4 | A     | cf009b51c9   | Clean cherry-pick. Compares shell executable file names case-insensitively on Windows, so `CMD.EXE` is no longer detected as PowerShell.                                                                                                                                                               |
| 1d76f828ee | B     | 0c7925aa3b   | Splits `ImageFormat` into `Svg` and `Raster`, records an SVG's intrinsic size through `usvg` when the bytes are not a known raster format, and loads such content as a GPUI SVG image. The remote markdown preview test was omitted because ZZZ has no `project_image_source`.                         |
| bc6fc11740 | A     | 082613b1e4   | Clean cherry-pick. Creates the DirectComposition target with `topmost = false`, so child windows such as a WebView2 hosted with wry draw above GPUI content on Windows.                                                                                                                                |
| 1e77078b3a | B     | 7d4ee7d44e   | Creates the session and installs the crash handler only after the single-instance check, so a second launch no longer overwrites the session ID before exiting. Upstream's telemetry-gated crash wrapper was omitted; ZZZ keeps its own `crashes::init`.                                               |
| 6569db7470 | A     | d37970bd67   | Clean cherry-pick. Tokenizes an SSH command line before dropping a leading `ssh` token, so shell-quoted executables such as `'ssh' '-p' '64042' 'user@127.0.0.1'` parse as remote destinations.                                                                                                        |
| 9e23c1f43e | C     | --           | Loads the c15t consent bundle from a hosted Inth CDN and drops the vendored copy. ZZZ removed the analytics script, the consent banner, and the c15t bundle, so there is no surface to update and the change would reintroduce a telemetry-adjacent path.                                              |
| 0967dd028c | A     | ccfbdc0ce6   | Clean cherry-pick. Captures `lambda_specifier` in the C++ highlights query, highlights `defined` as a preprocessor keyword and its argument as a macro constant, and adds a capture regression test.                                                                                                   |
| 1e47770804 | A     | 93ec9bf604   | Clean cherry-pick. Lets CodeLLDB configurations that use `build` omit `request`, and applies launch/attach target checks only when `request` is present, removing the false schema warnings.                                                                                                           |
| 7a349ce42d | A     | d74c0eb493   | Clean cherry-pick. Disables workspace matching when restoring remote workspaces, so saved windows sharing a directory stay separate instead of merging into one workspace.                                                                                                                             |
| e38d4ec980 | A     | b7a229de03   | Clean cherry-pick. Selects a remaining repository when the active Git repository is removed from a remote workspace, keeps the selection when another repository is removed, and clears it when the last one goes. Includes the upstream regression test.                                              |
| 4ec98df500 | C     | --           | Moves `agent-client-protocol` from 3.6.0 to 5.0.0 and rewrites the v1/v2 clients for the new schema layout. ZZZ pins `=3.5.0` and lacks the unabsorbed v1/v2 compatibility layer the patch builds on; the cherry-pick conflicted across 37 files and also touches native-agent tool files ZZZ deleted. |

Totals: seven `A` (all clean cherry-picks), seven `B`, three `C` across
seventeen commits.

### Applied Work

- `2f6d743bd1` is a `B` port of `a4b39c38e0`.
- `ed2e8d7752` is a `B` port of `d94f32a1d9`.
- `db39e5238a` is a `B` port of `c408f8efc4`.
- `13f9aaadae` is a `B` port of `f816e1f926`.
- `b3ad800004` is a `B` port of `decd2399b1`.
- `cf009b51c9` is a clean `A` cherry-pick of `b41a8b60a4`.
- `0c7925aa3b` is a `B` port of `1d76f828ee`.
- `082613b1e4` is a clean `A` cherry-pick of `bc6fc11740`.
- `7d4ee7d44e` is a `B` port of `1e77078b3a`.
- `d37970bd67` is a clean `A` cherry-pick of `6569db7470`.
- `ccfbdc0ce6` is a clean `A` cherry-pick of `0967dd028c`.
- `93ec9bf604` is a clean `A` cherry-pick of `1e47770804`.
- `d74c0eb493` is a clean `A` cherry-pick of `7a349ce42d`.
- `b7a229de03` is a clean `A` cherry-pick of `e38d4ec980`.

Every clean `A` used `git cherry-pick -x -s` and keeps the upstream
patch and message intact.

### Per-Commit Narrative

#### a4b39c38e0 — markdown preview image corners (B)

Upstream adds `rounded_images: bool` to `MarkdownStyle` with a `true`
default and sets it to `false` in `with_preview_overrides`, so the
preview's rendered images draw square. ZZZ builds its preview image
element through `modify_current_div` instead of upstream's helper, so the
cherry-pick conflicted and the two-line behavior change was applied by
hand in `crates/markdown/src/markdown.rs`. Agent-panel rendering keeps
the rounded default.

#### d94f32a1d9 — terminal tool header corners (B)

The upstream commit introduces a `TerminalToolHeader` component with a
`rounded_corners` builder and moves the header background out of
`render_collapsible_command`. ZZZ renders that header inline in
`crates/agent_ui/src/conversation_view/thread_view.rs` with no component
to hang the builder on, so the port applies the same rule there: round
the top corners in card layouts, round the bottom only when nothing is
rendered below, and let the terminal and generic tool cards paint the
header background.

#### c408f8efc4 — App borrow diagnostics (B)

Borrow failures used to panic with an ad-hoc message. The port replaces
`AppCell::borrow_failed` with an `AppBorrowError` value that records the
attempted caller location, the borrow kind, and the borrow-group origin
already tracked for diagnostics, and implements `Display`/`Debug` once
so both `borrow` and `try_borrow_mut` report the same detail.
`AsyncWindowContext::update`/`update_root` and
`AppContext::update_window` gained `#[track_caller]` so the reported
caller is the user of the API rather than the helper.

The `window.rs` half of the upstream commit adds operation context to
`log_err` sites in regions that have diverged: ZZZ routes accessibility
events through an `mpsc` channel, uses `retain(&(), ..)` callbacks, and
has no external-drag promotion or pending-input timeout machinery. Those
hunks were omitted; the retained borrow diagnostics do not depend on
them.

#### f816e1f926 — macOS screenshot rendering (B)

Upstream fixes a deadlock when taking a window screenshot while the
display link is pacing: the old path took a drawable from the layer,
which the display link already holds. The fix renders into a texture the
renderer owns. ZZZ's `MetalRenderer::render_scene_to_image` already
allocates and blits an owned texture, so the port reduces
`render_to_image` to a delegation at the layer's drawable size. The
upstream `render_offscreen` helper and its new Metal tests were omitted:
ZZZ paces macOS frames with `CVDisplayLink` rather than
`CAMetalDisplayLink`, and the intermediate-texture reuse and zero-size
early return the helper adds are already present locally.

#### 26abd1c959 — benchmark harness (C)

The commit is benchmark-only infrastructure: it grows the
`gpui_platform` benches, rewrites the randomized element tree fixture,
and adds `script/gpui-bench-compare`. Its only new GPUI APIs,
`Window::rendered_scene` and `BenchWindowContext::simulate_resize`, exist
behind the `bench-support` feature and have no caller outside the new
benches. ZZZ has no `crates/gpui_platform/benches`, no
`randomized_element_tree` module, and no bench targets to extend, so the
whole change is upstream-only tooling.

#### decd2399b1 — cached-view subscriptions (B)

Entities read through a cached view lost their subscriptions when the
cached output was reused, because reads were recorded against whichever
view was painting rather than the view that originally performed them.
The port adds an entity-read scope stack to `App`: a cached view pushes a
scope during prepaint and paint and records exactly its own reads, and a
deferred draw stores the reads from prepaint and paint and restores them
when that prepaint is reused. `extend_accessed` skips entities released
since the read was recorded, and the window takes the frame's read set
through `take_accessed`/`restore_accessed`.

The upstream `View`/`ViewElement` paint-path changes were omitted because
ZZZ keeps its `AnyView` caching shape; the capture lives in
`AnyView::paint`. `DeferredDraw` is defined in `crates/gpui/src/frame.rs`
locally, so the new field went there and the deferred test calls
`AnyView::cached` rather than `Entity::cached`.

#### 1d76f828ee — SVG in the project image store (B)

`ImageFormat` becomes `Svg` or `Raster(RasterImageFormat)`.
`ImageItem::compute_metadata_from_bytes` tries the known raster formats
first and falls back to `usvg` for anything else, recording the SVG's
intrinsic size, and `create_gpui_image` loads SVG content through the
GPUI SVG image path instead of failing. The image viewer reports the new
`SVG` format while keeping its translated labels and byte-size text. The
integration test covers opening an SVG by contents and path.

The upstream remote-markdown-preview test and the `project_image_source`
plumbing it exercises were omitted; ZZZ has no `project_image_source`,
and its Markdown preview resolves images through
`ImageSource::Resource`.

#### 1e77078b3a — session creation ordering (B)

A second launch used to write a fresh session ID before the
single-instance check rejected it, so the next cold start restored an
empty workspace instead of the previous session. The port moves the
`Session::new` spawn and the crash-handler installation below the check,
keeping them together. Upstream wraps the crash handler in a
telemetry gate; ZZZ has no telemetry, so it keeps its existing
`crashes::init` call unchanged and only relocates it.

#### 9e23c1f43e — docs consent bundle (C)

Upstream replaces the vendored `c15t@2.0.0-rc.3.js` with a script loaded
from Inth's hosted backend and updates `docs/theme/analytics.js`,
`book.toml`, the Prettier ignore, and the typo exclusions to match. ZZZ
removed `docs/theme/analytics.js`, the consent banner markup, and the
vendored bundle: there is no consent runtime to point at a CDN, and
loading one would reintroduce an analytics-adjacent surface the fork
deliberately does not ship.

#### 4ec98df500 — agent-client-protocol 5.0.0 (C)

The upgrade moves `agent-client-protocol` from 3.6.0 to 5.0.0, relocates
schema types from `schema::v1`/`schema::v2` to `v1::schema`/`v2::schema`,
switches builders to `Client.v1()`, and rewrites both ACP clients plus
their session-update, config-option, and elicitation code around the new
SDK. ZZZ pins `=3.5.0` in the root manifest and has not absorbed the
v1/v2 compatibility layer those rewrites sit on (`MaybeUndefined`,
`acp_v1::SessionUpdate`, `update_usage`,
`upsert_session_message_inner`, `append_session_message_chunk`,
`config_options::from_v1`). A dry-run cherry-pick conflicted across 37
files, and the patch also edits native-agent tool files ZZZ deleted. Per
the isolation test, it depends on unabsorbed earlier commits, so it is C
until those land.

### Verification

| Check                                                                                                                                                                      | Result  | Notes                                                    |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------- | -------------------------------------------------------- |
| `cargo check --all-targets -p markdown -p agent_ui -p gpui -p project -p image_viewer -p util -p remote -p languages -p dap_adapters -p remote_server -p workspace -p zzz` | PASS    | after the final commit                                   |
| `cargo test -p gpui --lib`                                                                                                                                                 | PASS    | 288 tests                                                |
| `cargo test -p util --lib shell`                                                                                                                                           | PASS    | 19 tests, including the case-insensitive detection cases |
| `cargo test -p project --test integration image_store`                                                                                                                     | PASS    | 6 tests, including `test_open_svg_image_by_contents`     |
| `cargo test -p remote --lib parse_command_line`                                                                                                                            | PASS    | 1 test, quoted and unquoted ssh executables              |
| `cargo test -p languages --lib cpp_highlights`                                                                                                                             | PASS    | 1 test, lambda and `defined` captures                    |
| `cargo fmt --all -- --check`                                                                                                                                               | PASS    | clean                                                    |
| `git diff e730f1f24f..HEAD --check`                                                                                                                                        | PASS    | clean                                                    |
| macOS / Windows runtime checks                                                                                                                                             | NOT RUN | host is Linux; those backends are platform-only          |
| `cargo test --workspace`                                                                                                                                                   | NOT RUN | narrowest per-crate checks were used                     |
| Remaining commits through the queried head                                                                                                                                 | NONE    | `4ec98df5000b` is the live head                          |

The reviewed baseline advances to
`4ec98df5000b21dca9aa1a064d770fb6ebc63880` for this batch, which is the
live upstream head at query time.
