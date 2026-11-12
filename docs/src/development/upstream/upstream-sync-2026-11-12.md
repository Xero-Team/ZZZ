---
title: Upstream Sync 2026-11-12
description: Selective Zed upstream sync, the 14 commits after the 2026-11-11 batch C.
---

# Upstream Sync 2026-11-12

## Scope

- Local base: `main` at `df0a452854`
- Target branch: `sync/upstream-2026-11-12`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `43bd5d3cacadd6087c237fe2843f260595b933a7`
- Live upstream head queried: `6ceb6fee8738274589b59139df27efc191ec2231`
- Query time: `2026-11-12`
- Reviewed range: `43bd5d3cac..6ceb6fee87`, 14 commits, oldest first
- New reviewed baseline: `6ceb6fee8738274589b59139df27efc191ec2231`
- Remaining after this batch: zero; the range ends at the live head

The whole 14-commit range fits in one batch. Eight of the commits are a
GPUI renderer series (`ba490197cf` through `8a5cf7e23a`) that builds on
upstream's shader layout and renderer architecture; ZZZ's GPU backends
keep Metal under `crates/gpui_macos/` rather than upstream's
`crates/gpui_apple/`, and the scene, window, and shader files have
diverged, so the series was classified commit by commit instead of
landing as a unit.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                                                                                                                                                    |
| ---------- | ----- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| ec6a4a4b5e | A     | 93793d2b10   | Parses the `DD` porcelain status as unmerged/both-deleted instead of both-added, with a test covering all seven unmerged codes. Cherry-picked intact; the only local divergence in `crates/git/src/status.rs` is outside the touched hunks.                                                                                    |
| ba06cc5191 | C     | --           | Adds the `title_bar.show` setting (`always` / `never` / `hide_in_full_screen`). Needs `Window::set_native_titlebar_visibility` and `FocusHandle::is_rendered`, neither of which exists locally, and reworks the title bar, sidebar, workspace, and window controls on all three platforms.                                     |
| 3d42ca7c09 | C     | --           | Adds `cx.debug_state` plus a `--debug-state` CLI flag. The snapshot reports cached-versus-platform pairs for visibility and frame scheduling; ZZZ's `Window` has no `WindowVisibility`, no `rendered_frame`, and no `needs_present` / `refreshing` / `next_frame_callbacks` fields.                                            |
| b6ff193ba6 | C     | --           | Remembers "Stay in Restricted Mode" and migrates the trust database to a `RemoteConnectionIdentity` key. Needs `RemoteConnectionKind`, `Project::remote_connection_identity`, and a migration over `trusted_worktrees.absolute_path`; ZZZ stores `absolute_path_v2`, keys trust by `RemoteHostLocation`, and has no such APIs. |
| f99c53f4f6 | C     | --           | Restores the last session when started by opening a path. Most of its 3231 inserted lines are a new restore flow inside upstream's `crates/zed/src/zed.rs`, which ZZZ keeps in `crates/zzz/src/main.rs` and `crates/zzz/src/zzz/open_listener.rs`; it also rewrites the persistence model.                                     |
| 3e23a0c482 | B     | 1f8ab9f62d   | Flips `file_finder.prefill_query_from_selection` to `false` and pins the old behaviour in the tests that assert prefill. Ported onto ZZZ's settings file and its two local seed-query tests; payload below.                                                                                                                    |
| 78f5d7446c | C     | --           | Defers the Windows `display_changed` callback to the foreground executor. ZZZ's Windows backend has no display-changed callback: `handle_display_change_msg` only stores the new display, so there is no re-entrant callback to defer.                                                                                         |
| ba490197cf | C     | --           | Moves content-mask clipping from hardware clip distances into the vertex shaders. Touches only the three shader files, all of which diverged locally (the WGSL file differs by 43/69 lines from the pre-image and binds storage at different groups).                                                                          |
| caead827d7 | C     | --           | Passes instance data to pixel shaders through interpolators. Depends on the previous commit's shader layout, which is not absorbed, and the local shaders have no matching pre-image (94/100 lines of drift in WGSL alone).                                                                                                    |
| ffe0fa29e4 | C     | --           | Replaces lyon tessellation and MSAA with a Slug-style winding path renderer. Adds `path_winding.rs`, rewrites `path_builder.rs`, `scene.rs`, `window.rs`, and all three renderers; the local copies of every one of those files diverged.                                                                                      |
| 1d8eda94bd | C     | --           | Improves primitive batching and removes the `bounds` argument from `Window::paint_layer`. ZZZ's `crates/gpui/src/window.rs` has no layer API at all, and `bounds_tree.rs` already differs from the pre-image by 40/41 lines.                                                                                                   |
| be07e43ea0 | C     | --           | Adds opaque and blended passes on the depth buffer and splits rounded quads in `paint_quad`. Builds on the batching and shader work above, and needs renderer files that diverged locally.                                                                                                                                     |
| 8a5cf7e23a | C     | --           | Swaps the Wallace shadow blur for Levien's approximation. A dry-run cherry-pick conflicts in all three shader files; the local shadow shader has neither `load_shadow` nor the inset-shadow fields the patch edits.                                                                                                            |
| 6ceb6fee87 | B     | 31fa615dfb   | Stops the branch picker from logging `entity released` when closed before its background requests finish by owning those tasks. Ported onto ZZZ's single default-branch task; payload below.                                                                                                                                   |

Totals: one `A`, two `B`, eleven `C`.

## Applied Work

- `93793d2b10` is a direct `A`: `git cherry-pick -x -s ec6a4a4b5e`.
- `1f8ab9f62d` is a `B` port of `3e23a0c482`, signed off for DCO.
- `31fa615dfb` is a `B` port of `6ceb6fee87`, signed off for DCO.

No remote, pull request, or named upstream remote was created.

## Per-Commit Narrative

### ec6a4a4b5e — unmerged `DD` status (A)

The commit maps the `DD` porcelain code to
`Deleted`/`Deleted` and adds `test_parse_unmerged_status_codes`, which
covers `DD`, `AU`, `UD`, `UA`, `DU`, `AA`, and `UU`. It applied without
conflict: the local file differs from the pre-image only in
`GitStatus::from_str` and in the removal of `DiffTreeType::MergeBaseWithWorktree`,
neither of which the patch touches.

### ba06cc5191 — `title_bar.show` (C)

The setting itself is philosophy-safe, but the patch is a cross-platform
title-bar rework rather than an isolatable behaviour:

- It adds `Window::set_native_titlebar_visibility` to the `PlatformWindow`
  trait and `FocusHandle::is_rendered` to GPUI. Neither exists locally.
- It changes `crates/title_bar`, `crates/sidebar`, `crates/workspace`,
  `crates/platform_title_bar` (including a `title_bar_settings.rs` that
  has no local counterpart), plus Wayland and X11 bounds observers and
  macOS window controls.
- ZZZ's `TitleBarSettingsContent` was rewritten after removing account UI,
  so the settings hunk has a different pre-image.

Porting it would mean re-authoring the title-bar layout on the platform
that cannot be tested here, so it is rejected rather than half-ported.

### 3d42ca7c09 — `--debug-state` (C)

`App::debug_state` itself would fit the local platform trait, but
`Window::debug_state` cannot be ported:

- It reports `visibility` as a cached/platform pair. ZZZ has no
  `WindowVisibility` type at all.
- It reports `needs_present`, `refreshing`, and `next_frame_callbacks`.
  Locally frame state lives in `Window::frame_scheduler`.
- It reads the rendered frame's cursor style; there is no `rendered_frame`
  on the local `Window`.
- Its test needs `simulate_visibility_change`, which is also absent.

### b6ff193ba6 — remembered restricted mode (C)

The behaviour is desirable and philosophy-safe, but the port needs
infrastructure that is not present:

- `RemoteConnectionKind`, used to serialise the identity columns.
- `Project::remote_connection_identity`; ZZZ exposes
  `remote_connection_options` and still keys trust by `RemoteHostLocation`.
- A migration over `trusted_worktrees.absolute_path`, while the local
  table is `trusted_worktrees(absolute_path_v2 BLOB, user_name, host_name)`.
- `RemoteConnectionIdentity::Docker` locally carries an extra `name` field,
  so the column mapping differs from upstream's.

`crates/git_ui_core/src/worktree_service.rs`, the file carrying most of
the propagation change, has no local counterpart; the equivalent module is
`crates/git_ui/src/worktree_service.rs` and it already diverged
(`TrustedWorktrees::try_get_global` instead of the static helper).

### f99c53f4f6 — restore session on path open (C)

The commit adds 3231 lines across seven files. The bulk is a session
restore state machine in upstream's `crates/zed/src/zed.rs`, with the CLI
plumbing in `crates/zed/src/main.rs` and `crates/zed/src/zed/open_listener.rs`.
ZZZ keeps those paths as `crates/zzz/src/zzz.rs`, `crates/zzz/src/main.rs`,
and `crates/zzz/src/zzz/open_listener.rs`, and the open path is
`handle_open_request` in `main.rs`. On top of that, the commit rewrites
`crates/workspace/src/persistence.rs`, `persistence/model.rs`, and
`workspace.rs`. This is an unisolatable rewrite of the startup path, not a
port.

### 3e23a0c482 — prefill default (B)

Upstream flips the default and adds a `FileFinderSettings::override_global`
block to each test that asserts prefill. Locally:

- `assets/settings/default.json` keeps the setting but in a different
  region of the file, so the flip was applied by hand.
- Only `test_seed_query_from_editor_selection` asserts prefill, so only it
  gained the override; `test_seed_query_from_selection_can_be_disabled`
  already covers the opt-out.
- The upstream tests `test_no_seed_query_without_selection` and
  `test_seed_query_flattens_multiline_selection` do not exist locally.
- `docs/src/reference/all-settings.md` does not document this setting in
  ZZZ, so there was no default to flip there.

### 78f5d7446c — Windows display change (C)

The panic came from invoking `display_changed` synchronously while GPUI was
already updating the window. ZZZ's Windows backend never invokes that
callback: `handle_display_change_msg` constructs the new `WindowsDisplay`,
stores it in `state.display`, and returns. Searching `crates/gpui` for
`display_changed` finds no callback registration at all, so there is no
re-entrant path to defer.

### ba490197cf, caead827d7, ffe0fa29e4, 1d8eda94bd, be07e43ea0, 8a5cf7e23a — GPUI renderer series (C)

The six commits are a pipeline of shader and renderer rewrites. They share
the same two blockers:

- The Metal shader lives at `crates/gpui_macos/src/shaders.metal` in ZZZ,
  not `crates/gpui_apple/src/shaders.metal`, and it has drifted from the
  pre-image of every commit in the series. The WGSL and HLSL shaders have
  drifted as well: ZZZ binds quads and shadows as storage buffers at
  `@group(1)`, and its shadow shader has no inset-shadow fields.
- Each commit builds on the previous one. Only the first (`ba490197cf`) has
  a pre-image that even resembles the local files, and even it needs a
  manual shader rewrite.

Beyond the shaders:

- `ffe0fa29e4` replaces the path renderer outright (`path_winding.rs`,
  `path_builder.rs`, `scene.rs`, `window.rs`, and all three backends) and
  makes `Path` non-generic. All of those files diverged locally.
- `1d8eda94bd` removes the `bounds` argument from `Window::paint_layer`.
  ZZZ's `crates/gpui/src/window.rs` has no layer API, so the changed
  call sites in `editor`, `markdown`, and `git_ui` have no local
  equivalent; `bounds_tree.rs` alone already differs by 40/41 lines.
- `be07e43ea0` splits rounded quads in `paint_quad` and adds depth passes to
  every backend, on top of the two commits above.

A dry-run `git cherry-pick` of `8a5cf7e23a` conflicted in all three shader
files, which is the recorded evidence for rejecting the series as a unit
rather than forcing the upstream shape.

### 6ceb6fee87 — branch picker entity released (B)

Upstream detaches two tasks and later gives the picker ownership of both.
ZZZ has only one of them (the default-branch request); the remote-URL
provider-icon task and its `maybe!` wrapper have no local counterpart
because ZZZ's picker loads no remote provider icons. The port keeps
`BranchList` owning the single task, so closing the picker drops it
instead of letting it update a released entity, and the `anyhow::Context`
import became unused and was removed.

## Verification

| Check                                                   | Result  | Notes                                                                      |
| ------------------------------------------------------- | ------- | -------------------------------------------------------------------------- |
| `cargo test --locked -p git`                            | PASS    | 66 passed, including `test_parse_unmerged_status_codes`.                   |
| `cargo test --locked -p file_finder seed_query`         | PASS    | 2 passed: the seeded test with the explicit override and the opt-out test. |
| `cargo check --locked -p git_ui`                        | PASS    |                                                                            |
| `cargo test --locked -p git_ui branch_picker`           | PASS    | 16 passed.                                                                 |
| `cargo check --locked --workspace`                      | PASS    | Whole workspace.                                                           |
| `cargo fmt --all -- --check`                            | PASS    |                                                                            |
| `git diff --check`                                      | PASS    |                                                                            |
| `cargo xtask check upstream-ledger`                     | PASS    | Ledger regenerated from this report.                                       |
| Windows and macOS rendering of the rejected shader work | NOT RUN | This host is Linux and none of that code was absorbed.                     |
