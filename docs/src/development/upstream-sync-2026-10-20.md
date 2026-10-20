---
title: Upstream Sync 2026-10-20
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-10-20

## Scope

- Target branch: `sync/upstream-2026-10-20`, branched from `main` at
  `039df53646`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `56934ec0560bce8929025497411de7f5322fb6ca`
- Reviewed head: `c4a82ab1ab958aa43b0c0b15c0bad4fb77a7ca7f`
- Live upstream head queried: `c4a82ab1ab958aa43b0c0b15c0bad4fb77a7ca7f`
- Query time: `2026-10-20T09:00:00+02:00`
- Reviewed range: every commit after `56934ec056`, oldest first
- Remaining after this batch: 0 commits through the queried live head

The previous reviewed baseline is an ancestor of `FETCH_HEAD`. Sixteen
commits landed upstream since the 2026-10-17 audit, so the whole range fits
in this batch.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                     |
| ---------- | ----- | ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| b4ecc45422 | B     | 4e22d1f625   | Groups sprites by atlas texture and colocates sort keys; WebGL shader hunk omitted because ZZZ has no WebGL backend.                            |
| da5921d6ba | C     | --           | Requires the ACP v2 session-state and usage-generation architecture that ZZZ does not carry.                                                    |
| 1f32ec0cea | C     | --           | Adds explicit ACP v2 session close on top of the absent v2 state machine.                                                                       |
| 32f70854ff | C     | --           | Stages pre-session ACP v2 updates; ZZZ has a flat v1-only `agent_servers/src/acp.rs` and no v2 notification pipeline.                           |
| febcb5bb99 | C     | --           | Dependency and lockfile bump for proptest plus test-only API adaptations, with no independent ZZZ behavior.                                     |
| 15591019b6 | B     | 11f83d6d7f   | Prefers suffix matches over first-stem matches in file icons, fixing `mod.rs` and `Dockerfile.rs`; adds the regression test.                    |
| afff3d2616 | C     | --           | Debugger hover tree depends on an upstream hover-popover rewrite and point-diagnostics extraction that ZZZ has not absorbed.                    |
| 042e0406d3 | A     | --           | Already equivalent: local `PathStyle::split` and `util::normalize_path` already provide the header behavior and helper location.                |
| faf96952a2 | B     | 87cfda6d20   | Restores IME composition underlines in single-line editors and keeps highlight underlines everywhere except the minimap.                        |
| 9cd14daad7 | C     | --           | Requires organization IDs for Cloud feedback, a hosted account surface ZZZ removes.                                                             |
| 39323b8179 | B     | 05be3c95a5   | Conversation dispatch now checks the current thread entity as well as the session ID; the v2-dependent regression test is omitted.              |
| 784e431c42 | C     | --           | Preserves ACP input through receipt-backed reconnect and full replay, architecture that depends on the rejected v2 session work.                |
| 60e8c18598 | B     | ccada40192   | Disambiguates Markdown preview tabs by detail level and shows parent directories; upstream test helpers absent locally, so tests are omitted.   |
| e39fb7c2e0 | C     | --           | Renames `ScrollWheelEvent` and adds iOS trackpad scrolling against `gestures.rs` and `element/mouse.rs`, files ZZZ has consolidated or removed. |
| 2c626b3397 | C     | --           | iOS-only Universal Link forwarding; ZZZ has no `gpui_ios` crate or iOS target.                                                                  |
| c4a82ab1ab | C     | --           | Adds iOS display corner radii and `on_back_gesture`; no local caller, no `gpui_ios`, and it builds on the rejected scroll rename.               |

Totals: one `A` (already equivalent, no code change), five `B`, ten `C`
across sixteen commits.

## Applied Work

- `4e22d1f625` is a `B` port of `b4ecc45422`.
- `11f83d6d7f` is a `B` port of `15591019b6`.
- `87cfda6d20` is a `B` port of `faf96952a2`.
- `05be3c95a5` is a `B` port of `39323b8179`.
- `ccada40192` is a `B` port of `60e8c18598`.

## Per-Commit Narrative

#### b4ecc45422 — sprite atlas batching (B)

`Scene::finish` now sorts mono, subpixel, and polychrome sprites by
`sprite.tile.texture_id.index` rather than `tile_id`, matching the texture
batches the renderer already emits. The `AtlasTile` field moves directly
after `order` and `pad` in the Rust records and in the wgpu and Windows
shader layouts, bringing the texture index to offset 8 without changing
record size. The WebGL shader hunk is omitted because ZZZ deleted
`shaders_webgl.wgsl`.

#### da5921d6ba — ACP foreground state and resume snapshots (C)

The commit rewrites usage-event semantics, adds generation-tagged
legacy/native presentation, introduces `received_session_state` and
`ReportedTokenUsage`, and teaches resume to tolerate missing state
snapshots. ZZZ's `AcpThread` has no `SessionActivity`, no session-state
ingestion, and imports only `agent_client_protocol::schema::v1`. The
required state machine is absent.

#### 1f32ec0cea — explicit ACP v2 session close (C)

Adds `CloseSessionRequest` handling and close bookkeeping to
`agent_servers/src/acp/v2.rs` and the v2 submission path. ZZZ has no
`acp/v2.rs`, no `V2Builder`, and a flat v1 ACP module, so the change
cannot be isolated.

#### 32f70854ff — preserve pre-session ACP updates (C)

Introduces `session_updates.rs`, pending-update staging, byte accounting,
and v2 admission generation tracking across `acp.rs`, `acp/runtime.rs`,
and `acp/v1.rs`. ZZZ's flat `acp.rs` only implements v1 and has none of
these modules or notification lanes.

#### febcb5bb99 — bump proptest (C)

Replaces the pinned proptest git revision with crates.io 1.12.0, updates
`Cargo.lock`, and adapts three test modules. This is dependency and
lockfile churn with test-only fallout and no independent ZZZ behavior.

#### 15591019b6 — file icon suffix precedence (B)

`get_icon` now walks every dotted suffix before falling back to the first
stem, so `mod.rs` resolves as Rust instead of the Go `mod` stem and
`Dockerfile.rs` resolves as Rust. The manual port dropped upstream's
pre-existing folder-indicator helpers and tests, which ZZZ does not carry,
and added the new test module with `gpui` and `theme` test-support
dev-dependencies.

#### afff3d2616 — debugger hover tree (C)

The commit adds `editor/debugger_hover.rs`, thousands of lines of
hover-popover changes, point-diagnostics extraction in `element.rs`, and
new project debugger methods. ZZZ's hover popover has diverged around
inlay hover ranges, i18n, and hiding-delay behavior, and it lacks the
upstream point-diagnostics refactor, so the feature cannot be ported
without an unisolatable rewrite.

#### 042e0406d3 — path helper cleanup (A, already equivalent)

Upstream moves `normalize_path` into `crates/util/src/paths.rs` and
removes `PathStyle::split` in favor of `file_name` plus suffix stripping.
ZZZ has no `crates/path`; `util::normalize_path` already exists locally,
and `PathStyle::split` already produces the same header directory and file
name in `render_buffer_header`. No local change is needed.

#### faf96952a2 — IME composition underline (B)

Highlight underlines are now enabled whenever the editor is not a
minimap, diagnostics visibility gates language-aware highlighting, and
disabling diagnostics forces the maximum severity to `Off`. The manual
port kept the local point-diagnostic rendering, exposed
`initialize_new_minimap` and `create_style` as `pub(crate)` for the
regression test, and omitted upstream's unrelated point-diagnostics
extraction and inlay hit-testing helpers.

#### 9cd14daad7 — Cloud feedback organization ID (C)

Requires an organization identifier when submitting feedback to Cloud and
adjusts `cloud_api_types` and edit-prediction call sites. Hosted account
and Cloud surfaces are absent in ZZZ.

#### 39323b8179 — replaced session owners (B)

Conversation event dispatch now checks that the thread in the `threads`
map is the same entity that emitted the event, rather than trusting a
matching session ID. This prevents a retained `ThreadView` from applying
stale-owner load errors or queue drains to a replacement session. The
upstream test depends on ACP v2 activity generations and stop reasons, so
it is omitted.

#### 784e431c42 — preserve input across ACP reconnect (C)

Builds receipt-backed submission recovery, full-replay restore, tombstones,
and a disconnected "Copy Saved Input" flow on top of the ACP v2 session
state introduced by the rejected commits. ZZZ has no receipt or replay
architecture.

#### 60e8c18598 — ambiguous preview tab titles (B)

`MarkdownPreviewView::tab_content_text` now asks the source editor for its
title at the requested detail level, and `tab_content` shows a muted
parent-directory description and truncates like editor tabs. The shared
description logic moved into `Editor::tab_description`. Preview content
keeps the existing i18n keys. The upstream tests rely on
`markdown_workspace` and `open_project_file` helpers that ZZZ does not
carry, so they are omitted.

#### e39fb7c2e0 — scroll event rename and iOS trackpad (C)

Renames `ScrollWheelEvent` to `ScrollEvent`, adds `ScrollSource`, and adds
`UIPanGestureRecognizer` handling on iOS. The patch edits
`crates/gpui/src/gestures.rs` and `crates/editor/src/element/mouse.rs`,
which ZZZ consolidated away, and it targets `gpui_ios`, which ZZZ does not
carry. The retained behavior would be iOS-only and has no local caller.

#### 2c626b3397 — iOS Universal Links (C)

Forwards Universal Links in `gpui_ios` and adds the associated-domain
dependency. ZZZ has no `gpui_ios` crate or iOS target.

#### c4a82ab1ab — iOS corner radii and back gesture (C)

Adds display corner radii, `on_back_gesture`, and iOS window plumbing. It
builds on the rejected scroll rename, needs the absent `gestures.rs` and
`gpui_ios`, and introduces public GPUI APIs with no current ZZZ caller.

## Verification

| Check                                                                                 | Result  | Notes                                                 |
| ------------------------------------------------------------------------------------- | ------- | ----------------------------------------------------- |
| Live `git ls-remote` and `git fetch --no-tags`                                        | PASS    | `FETCH_HEAD=c4a82ab1ab958aa43b0c0b15c0bad4fb77a7ca7f` |
| Previous baseline ancestry                                                            | PASS    | `56934ec056` is an ancestor of `FETCH_HEAD`           |
| `cargo check --locked -p gpui -p gpui_wgpu`                                           | PASS    | sprite layout and sort keys                           |
| `cargo test --locked -p file_icons test_suffix_matches_take_precedence_over_prefixes` | PASS    | regression test                                       |
| `cargo test --locked -p editor test_highlight_underlines_are_only_hidden_in_minimap`  | PASS    | regression test                                       |
| `cargo check --locked -p agent_ui`                                                    | PASS    | session-owner dispatch                                |
| `cargo check --locked -p markdown_preview`                                            | PASS    | tab title and description                             |
| `cargo fmt --check`                                                                   | PASS    | clean                                                 |
| `git diff --check`                                                                    | PASS    | clean                                                 |
| `./script/check-philosophy`                                                           | PASS    | philosophy gate                                       |
| Locale key parity (`en.json` vs `zh-CN.json`)                                         | PASS    | no new keys; existing `markdown_preview` keys reused  |
| Windows `gpui_windows` shader compile                                                 | NOT RUN | host is Linux                                         |
| macOS `gpui_macos` compile and runtime checks                                         | NOT RUN | host is Linux and no Darwin target is installed       |
| iOS `gpui_ios` checks                                                                 | NOT RUN | crate absent locally                                  |
| `cargo test --workspace`                                                              | NOT RUN | not requested                                         |

The general reviewed baseline advances to
`c4a82ab1ab958aa43b0c0b15c0bad4fb77a7ca7f`.
