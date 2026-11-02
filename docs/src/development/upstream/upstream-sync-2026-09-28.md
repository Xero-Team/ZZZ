---
title: Upstream Sync 2026-09-28
description: Targeted Zed upstream backport of the status-bar "Hide Button" UI.
---

# Upstream Sync 2026-09-28

## Scope

- Target branch: `sync/upstream-2026-09-28-backport-hide-button` from
  local `main` at `eb72ac540d`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Backported upstream commit: `c049193fd93b2c27d3e4a8a40f5e1739db4640b1`
- Live upstream head queried: `bda9c0bd43a8d235d82adb01ea5bc875b861ecfc`
- Query time: `2026-09-28`
- Reviewed baseline: unchanged at
  `bda9c0bd43a8d235d82adb01ea5bc875b861ecfc` (see
  `upstream-sync-2026-09-27.md`)

This run is a targeted backport, not a contiguous batch. The user asked
to absorb the ability to hide individual status-bar and dock panel
buttons. Upstream implemented that in PR
[#54971](https://github.com/zed-industries/zed/pull/54971), squashed into
`c049193fd93b2c27d3e4a8a40f5e1739db4640b1` ("Make all status bar tools
able to hide its button via UI"). That commit is an ancestor of the
current reviewed baseline but predates the earliest tracked report range
boundary (`2026-06-20`, see `upstream-sync-2026-07-03.md`), so it had
never been classified. Backporting it does not advance the reviewed
baseline.

## Decisions

| Upstream | Class | Local commit | Disposition                                                                                                                                  |
| -------- | ----- | ------------ | -------------------------------------------------------------------------------------------------------------------------------------------- |
| c049193f | B     | 2de3161a49   | `HideStatusItem` / `hide_button_setting` plus "Hide Button" context-menu entries; collab and edit-prediction hunks omitted, label localized. |

Totals: zero `A`, one `B`, zero `C`.

## Applied work

No clean `A` was available. `c049193f` conflicts with local divergence
and touches rejected or absent surfaces, so it was ported as a single `B`
commit with `git commit -s` and `sync:` / `Upstream:` / `Retained:` /
`Omitted:` trailers.

`2de3161a49` retains:

- `crates/workspace/src/status_bar.rs`: a `HideStatusItem` wrapper around
  a `Fn(&mut SettingsContent)` that persists the change with
  `update_settings_file`; a required `StatusItemView::hide_setting`; a
  `StatusItemViewHandle::hide_setting`; `render_hideable_item`, which
  wraps every left/right status item in a right-click menu when the item
  supplies a hide setting; and `add_hide_button_entry`.
- `crates/workspace/src/dock.rs`: a defaulted
  `Panel::hide_button_setting`, its `PanelHandle` counterpart, a
  "Hide Button" entry appended to each dock panel button's context menu,
  and a `None` `hide_setting` for `PanelButtons`.
- Every local `StatusItemView` and `Panel` implementor now returns the
  matching `button: false` setting (or an explicit `None` for items whose
  visibility is already conditional).
- `crates/workspace/src/workspace.rs` re-exports `HideStatusItem` and
  `EncodingDisplayOptions`.

Local adaptations for ZZZ divergences:

- The `collab_ui` hunk is omitted: collab is absent in ZZZ.
- The `edit_prediction_ui` hunk is omitted: the crate is absent in ZZZ.
- `add_hide_button_entry` takes an extra `cx: &App` so the entry reads
  `i18n::tr(cx, "workspace.status_bar.hide_button", "Hide Button")`; both
  `assets/locales/en.json` and `assets/locales/zh-CN.json` gained that
  key. It is not re-exported because only `dock.rs` calls it.
- New `None` implementations were added for the ZZZ-only
  `audio_viewer::AudioInfo` and `video_viewer::VideoInfo` status items.

## Per-commit narrative

### c049193f — B, `2de3161a49`

A dry-run `git cherry-pick` was not attempted because the commit is a
squashed upstream merge whose parent tree predates many local
divergences; the port was written directly against current ZZZ APIs.

The commit's collab hunk mutates
`settings.collaboration_panel...button`, which cannot exist locally, and
its edit-prediction hunk lives in a crate ZZZ does not ship. Both are
dropped; neither is required for the retained hide mechanism, which is
testable per item through the new `hide_setting` return value.

The remaining philosophy-safe behavior — the user can right-click a
status-bar item or a dock panel button and choose "Hide Button", which
writes the item's own `button` setting — is retained intact. This is
ACP-only-safe: it configures which local UI elements are visible and
does not introduce any account, collab, telemetry, or native-agent
surface.

## Verification

| Check                                            | Result  |
| ------------------------------------------------ | ------- |
| `git diff --check`                               | PASS    |
| `cargo fmt --all` (only touched files changed)   | PASS    |
| `cargo check --locked -p workspace`              | PASS    |
| `cargo check --locked -p workspace --tests`      | PASS    |
| `cargo check` on all 19 touched dependent crates | PASS    |
| Locale key parity (`en.json` vs `zh-CN.json`)    | PASS    |
| `./script/check-philosophy`                      | PASS    |
| Manual macOS / Windows menu interaction          | NOT RUN |
| `cargo test --workspace`                         | NOT RUN |

The status-bar and dock context-menu behavior was verified at the type
and borrow-checking level only; the GPUI menu interaction itself has no
automated test locally.
