---
title: Upstream Sync 2026-11-22
description: Selective Zed upstream sync, seven commits after 54fff2ff5b.
---

# Upstream Sync 2026-11-22

## Scope

- Local base: `sync/upstream-2026-11-22` from `main` at `e824f4608c`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `54fff2ff5b2f9fdbe1b99c0c2566e15dcb6cfcb9`
- Reviewed range: `54fff2ff5b..f4fe0be1f8`, 7 commits, oldest first
- New reviewed baseline: `f4fe0be1f8c7974db07e01c5cd05bee0cf4eb8d9`
- Live head at query time: `f4fe0be1f8c7974db07e01c5cd05bee0cf4eb8d9`
- Remaining after this batch: none

Batch E drained the queue up to `54fff2ff5b`, so this is the next window
and it is short: seven commits, of which three describe local code and
four target crates or platform APIs this tree does not carry. Both ports
needed the local shape rather than the upstream one: `git_ui` localizes
the pull-request labels, and `terminal_element::cell_style` takes an
`IndexedCell`.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                              |
| ---------- | ----- | ------------ | ---------------------------------------------------------------------------------------------------------------------------------------- |
| 852edce6f8 | B     | ccc860f057   | Routes the post-push pull request link through the `create pull request` URL builder, with the remote's link as fallback. Payload below. |
| 4f5e323e25 | C     | --           | Needs `tooling/crates_io_client` and wires up crates.io publishing, which is absent here: every crate sets `publish = false`.            |
| 167f53b58f | B     | 1835577025   | Caches the terminal minimum contrast result per color pair and skips the check at `minimum_contrast` 0. Payload below.                   |
| 2f808b3c2d | A     | a19c206aef   | Clean cherry-pick; min and max anchors resolve without a rope walk.                                                                      |
| 3b71d0c834 | C     | --           | `crates/sandbox_linux` and its broker are absent locally, so the permissive `mkdir` rule has no target.                                  |
| 8ad65dee1c | C     | --           | `crates/google_ai` is absent locally, and no local provider shares the tool-call id generator the patch edits.                           |
| f4fe0be1f8 | C     | --           | Adds GPUI `NativeView`; it needs `crates/gpui_apple`, which local does not have, and no local caller would use the new element.          |

Totals: one `A`, two `B`, four `C`.

## Applied Work

- `2f808b3c2d` used `git cherry-pick -x -s` against the clean tree and
  kept the upstream message.
- `852edce6f8` and `167f53b58f` both started as cherry-picks, conflicted,
  and were aborted and re-ported as `B`; each carries the
  `Upstream:`/`Retained:`/`Omitted:` payload below.

No named upstream remote, push, or pull request was created.

## Per-Commit Narrative

### 852edce6f8 — one pull request URL builder for command and toast (B)

Retained: the post-push toast now asks `GitPanel::create_pull_request_url`
for its link, the same builder the `git: create pull request` command
uses, and only falls back to the URL the remote printed when that builder
fails. `PULL_REQUEST_HINTS` carries a `PullRequestLinkKind`, so
`SuccessStyle::PushPullRequestLink` splits into
`CreatePullRequest { label, fallback_url }` and
`OpenPullRequest { label, url }`; a "View merge request" link still opens
the remote's URL because only the remote resolves an existing request.
`create_pull_request` becomes a thin wrapper that reports the error toast,
and `git_ui` gains `url`.

Omitted: the hints keep the local `i18n::tr` lookup
(`label_key`/`label_fallback`) and their labels stay `String` rather than
`&'static str`, because the local `extract_pull_request_link` takes a
`translate` closure. The push message keeps the localized
`git_ui.remote_output.pushed_to` string instead of upstream's
`format!("Pushed {} to {}")`. The cherry-pick conflicted on all of those,
which is why it became a `B`; a dry run that forced upstream's shape
would have dropped localization.

### 167f53b58f — memoized terminal contrast (B)

Retained: `layout_grid` creates one `HashMap<(Hsla, Hsla), Hsla>` per
frame and passes it to `cell_style`, which stores the
`ensure_minimum_contrast` result per foreground and background pair
instead of recomputing it for every visible cell. The check is skipped
when `minimum_contrast` is 0, so that setting does not pay for a lookup
per cell. `test_cell_style_caches_contrast_per_color_pair` covers named
and indexed pairs, both cache directions, and the 0, 45 and 90
thresholds.

Omitted: nothing. The port keeps the local `cell_style` signature, which
takes an `IndexedCell` and reads `indexed.c` rather than upstream's `Cell`
plus `cell.character()`, and the test builds an `IndexedCell` and imports
`Color`/`NamedColor` from `terminal::alacritty_terminal::vte::ansi`
because this crate does not re-export them at the root.

### 4f5e323e25 — crates.io client in the GPUI publishing xtask (C)

The patch adds `crates_io_client = { path = "tooling/crates_io_client" }`
to the workspace and depends on it from `tooling/xtask`. Local has no
`tooling/crates_io_client`, and the only publishing surface here is the
`publish-gpui` command, whose packages all set `publish = false`. This is
upstream release plumbing with no local behavior to check, and it would
add a network client and a lockfile delta for a path ZZZ does not ship.

### 3b71d0c834 — more permissive `mkdir` in `sandbox_linux` (C)

The commit edits `crates/sandbox_linux/src/broker.rs` and its
`examples/end_to_end.rs`. There is no `sandbox_linux` crate here, and no
local sandbox broker to relax.

### 8ad65dee1c — globally unique tool call IDs (C)

The commit edits `crates/google_ai/src/completion.rs`. `crates/google_ai`
is absent locally, and no local provider carries the shared tool-call id
generator the patch rewrites.

### f4fe0be1f8 — GPUI `NativeView` on macOS (C)

The commit is 3648 lines across `crates/gpui`, `crates/gpui_apple`,
`crates/gpui_macos` and the Windows and wgpu renderers, and it introduces
the element itself: scene compositing hooks, input routing, cursor
handling and an accessibility path. `crates/gpui_apple` does not exist
locally, and no local caller would place a platform view in a window, so
this fails the isolation test twice over. It also ships two examples that
only run on macOS. This is a feature that needs its own batch and a local
consumer, not a port.

## Verification

| Check                                                                        | Result  | Notes                                                                                    |
| ---------------------------------------------------------------------------- | ------- | ---------------------------------------------------------------------------------------- |
| `cargo check -p git_ui`                                                      | PASS    | After the `852edce6f8` port; also refreshed `Cargo.lock` for the new `url` dependency.   |
| `cargo test -p git_ui remote_output::tests`                                  | PASS    | 5 passed, including the new `CreatePullRequest` and `OpenPullRequest` shapes.            |
| `cargo test -p terminal_view test_cell_style_caches_contrast_per_color_pair` | PASS    | 1 passed, 73 filtered out.                                                               |
| `cargo test -p text --lib summaries_for`                                     | PASS    | 2 passed: `test_summaries_for_unordered_anchors` and its `_across_chunks` variant.       |
| `cargo check --locked --workspace`                                           | PASS    | Warnings only, all pre-existing manifest warnings.                                       |
| `cargo fmt --all -- --check`                                                 | PASS    |                                                                                          |
| `cargo xtask check upstream-ledger`                                          | PASS    | Ledger regenerated from this report's decision table.                                    |
| Prettier over this report                                                    | PASS    | `3.9.10`, the version pinned by `script/prettier`.                                       |
| macOS and Windows runtime behavior                                           | NOT RUN | This host is Linux. The `NativeView` hunks and the renderer changes were only inspected. |
