---
title: Upstream Sync 2026-09-27
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-09-27

## Scope

- Target branch: `sync/upstream-2026-09-27` from local `main` at
  `8175623e1cb9`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previously reviewed baseline: `933d8d93819c749a607e561883855a9b95c79cea`
- Reviewed upstream head: `bda9c0bd43a8d235d82adb01ea5bc875b861ecfc`
- Live upstream head queried: `bda9c0bd43a8d235d82adb01ea5bc875b861ecfc`
- Query time: `2026-09-27T16:57:50+02:00`
- Requested range starts after `933d8d93819c749a607e561883855a9b95c79cea`

Upstream `main` had only three commits after the previous baseline, so
this run reviews all of them, from `64406cc2a8` through `bda9c0bd43`.
The reviewed baseline advances to
`bda9c0bd43a8d235d82adb01ea5bc875b861ecfc`, which is also the live head
at query time.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                            |
| ---------- | ----- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 64406cc2a8 | B     | b83ab8dd7f   | Cross building the Windows remote server from Unix uses cargo-xwin; local path and macOS/zig divergences kept.                                         |
| 70e686c2a3 | C     | --           | Feature upsell rendering optimization targets upsell machinery that is absent locally; rejected on philosophy grounds.                                 |
| bda9c0bd43 | B     | 42b04a8cb7   | Aligned wrapped lines get correct paint-layer bounds; the markdown highlight reordering needs the absent `RenderedLineElement` and new GPUI paint API. |

Totals: two `B`, one `C`, zero clean `A`.

## Applied work

No clean `A` was available, so no direct `git cherry-pick -x -s` commit
was created. Every `B` was committed with `git commit -s` and a `sync:`
subject with `Upstream:` / `Retained:` / `Omitted:` trailers. No remote,
pull request, or named upstream remote was created.

- `b83ab8dd7f` ports the `cargo-xwin` cross-build route for a Windows
  remote server, adding a `RemoteServerBuildMode` enum and an
  `ensure_rustup_target` helper around the local build divergences.
- `42b04a8cb7` ports the `line_paint_bounds` extraction from
  `bda9c0bd43` and its `test_aligned_line_paint_bounds` regression test.

## Per-commit narrative

### 64406cc2a8 — B, `b83ab8dd7f`

A dry-run `git cherry-pick` conflicted in
`crates/remote/src/transport.rs` only. The port keeps upstream's
behavior: the Windows triple is always `pc-windows-msvc`, a Windows
remote on a non-Windows host selects the `xwin` build mode, and that mode
first checks `clang` and `cargo-xwin`, adds the rustup target and the
`llvm-tools` component, then runs `cargo xwin build`. The conflict came
from local divergence, which the port preserves:

- `util::dev_repo_root()` is replaced with the local
  `concat!(env!("CARGO_MANIFEST_DIR"), "/../..")` path.
- Upstream's single trailing `run_cmd` still goes through the local
  `ZZZ_BUILD_REMOTE_SERVER`, `ZZZ_ZSTD_MUSL_LIB`, macOS SDKROOT
  preparation, and the local `apply_tmpfs_zig_cache` for Zig mode.
- The native and Zig paths keep their existing status messages.

`cargo check --locked -p remote` passes.

### 70e686c2a3 — C

Rewrites `render_feature_upsell_banner` and `render_feature_upsells`
in `crates/extensions_ui/src/extensions_ui.rs` to remove allocations and
use iterator size hints. The local `extensions_ui` has no
`render_feature_upsells`, no `Feature` upsell enum, and no upsell strings
at all; these surfaces were deliberately removed. There is nothing to
optimize locally, and restoring the upstream code to absorb the
optimization would reintroduce the promotional external-agent and
language upsell banners. This is a philosophy rejection against an absent
surface.

### bda9c0bd43 — B, `42b04a8cb7`

Fixes Markdown Preview search highlights hiding text under opaque theme
colors. The fix has two separable parts:

- `crates/gpui/src/text_system/line.rs` extracts `line_paint_bounds` and
  uses it in `paint_line` and `paint_line_background`. The old bounds did
  not account for center or right alignment, so the paint layer of a
  wrapped, aligned line could clip its rows. The local file matched the
  pre-commit shape exactly, so this part applied as a normal bugfix on an
  existing path, with upstream's `test_aligned_line_paint_bounds`
  ported unchanged. `cargo test --locked -p gpui --lib
test_aligned_line_paint_bounds` passes.
- The markdown half of the commit rewires `RenderedLineElement` to paint
  run backgrounds, then highlights, then glyphs, and adds
  `TextLayout::paint_background` / `paint_foreground`. Local markdown has
  no `RenderedLineElement`; it paints search highlights and selection at
  the `MarkdownElement` level in `paint`, and its per-line alignment
  handling has already diverged. Porting the reordering would require
  importing the new `TextLayout` public API, which would then have no
  local caller, and rebuilding the per-line element architecture is a
  rewrite rather than a port. Those hunks are omitted; the retained
  `line_paint_bounds` invariant does not depend on them.

## Verification

| Check                                                                         | Result  |
| ----------------------------------------------------------------------------- | ------- |
| `git diff --check`                                                            | PASS    |
| `cargo fmt --all -- --check`                                                  | PASS    |
| `cargo check --locked -p remote` (cargo-xwin port)                            | PASS    |
| `cargo check --locked -p gpui` (line paint bounds)                            | PASS    |
| `cargo test --locked -p gpui --lib test_aligned_line_paint_bounds` (1 passed) | PASS    |
| Cross-build runtime behavior for xwin / zig modes                             | NOT RUN |
| macOS / Windows runtime checks for platform hunks                             | NOT RUN |
| `cargo test --workspace`                                                      | NOT RUN |

The xwin runtime route was not exercised because it needs `cargo-xwin`,
`clang`, and the rustup Windows target on the host; only the Rust type
and borrow checking of the changed code is verified.
