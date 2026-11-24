---
title: Upstream Sync 2026-11-24
description: Selective Zed upstream sync, twenty commits after f4fe0be1f8.
---

# Upstream Sync 2026-11-24

## Scope

- Local base: `sync/upstream-2026-11-24` from `main` at `e3ca693341`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `f4fe0be1f8c7974db07e01c5cd05bee0cf4eb8d9`
- Reviewed range: `f4fe0be1f8..f00a94318c`, 20 commits, oldest first
- New reviewed baseline: `f00a94318c059f03892e04f82343e899fbfaa844`
- Live head at query time: `f98c2c100d217ace3ca99564cfc4573612d43d47`
- Remaining after this batch: 45

Batch F is the first window after the ACP-only cleanup settled, and it
shows what that cleanup costs: sixteen of the twenty commits touch a
surface this tree no longer carries. Six patch the in-tree model
providers (`google_ai`, `bedrock`, `copilot_chat`,
`language_model_core/src/chat_completion.rs`), four are ACP v2, two
build on the rejected GPUI `NativeView` layer, two patch the git graph at
its upstream `git_ui` path, and one each touch `gpui_web`'s deleted
viewport module, the assets of `crates/icons`, and a wgpu renderer that
has diverged by more than a thousand lines. Two ports needed the local
shape: the graph-facing `RenderGlyphParams` keeps its synthetic
italic/bold fields, and the dev-container restart path has no compose
branch.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                        |
| ---------- | ----- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 5e3cf96193 | C     | --           | Adds long decode support to the absent `crates/google_ai`; there is no `language_models` provider crate here either.                                               |
| 29de395fea | C     | --           | Relative `./` and `../` search across six files; the pick conflicted in five, and the invariant spans `remote_servers` and `toolchain_selector` divergences.       |
| d31d1074a0 | C     | --           | Second half of the rejected GPUI `NativeView` layer: `gpui/src/window/native_view.rs` and `gpui_macos/src/window/native_views.rs` are absent locally.              |
| f126950f9d | C     | --           | Superseding a reconnecting client, but the mock transport lost `proxy_tx` and `reported_options` and `remote_connections` now uses `WorkspaceOpenTarget`.          |
| f621145963 | C     | --           | Adds Claude Sonnet and Haiku models to the absent `crates/bedrock`.                                                                                                |
| 9935ef714a | C     | --           | Adds GPUI `text_scale_factor` plumbing with no caller in ZZZ or upstream; local `App` also keys observers through `SubscriberSet`, not `SingleSubscriberSet`.      |
| b699054057 | C     | --           | Parses integer error codes in `language_model_core/src/chat_completion.rs`, which does not exist locally.                                                          |
| aee0381dee | C     | --           | Copilot model picker fallback; `crates/copilot_chat` is absent and Copilot routes are rejected by policy.                                                          |
| 11c07191f2 | C     | --           | iOS keyboard occlusion; the patch rewrites `gpui_web/src/viewport.rs`, which local deleted, and ZZZ has no iOS backend.                                            |
| d3bba51492 | C     | --           | Aligns ACP schema 1.21.2 inside the absent upstream `acp/v1.rs` and `acp/v2.rs` split; local `agent_servers/src/acp.rs` is one v1-only module with no `v2::` call. |
| 2fc98e1e9b | C     | --           | Renders ACP v2 option descriptions as Markdown; there is no local v2 path to render them on.                                                                       |
| d551a5fd50 | B     | d48b2daccb   | Hashes signed zero canonically in `Pixels`, `RenderGlyphParams` and `DocumentColor`. Payload below.                                                                |
| 9bf75d5434 | C     | --           | Preserves git graph selection across refreshes, but the graph lives in the local `git_graph` crate with a diverged selection and load path.                        |
| b77f61f3e3 | C     | --           | Drops eight icons from `crates/icons`, four of which local still uses: `Attach`, `BookCopy`, `DatabaseZap` and `TextSnippet`.                                      |
| 9d1a1bf541 | B     | 6fb3f98704   | Runs `initializeCommand` before restarting a stopped dev container. Payload below.                                                                                 |
| 78edf1df97 | A     | --           | Already equivalent: local `ThreadMetadata` has no `title_override`, so search already matches the displayed title.                                                 |
| d5b6a7f1ec | C     | --           | Graph selection colors need `ContextMenu::target_entry_index` and a focusable table state, neither of which the local `ContextMenu` has.                           |
| ac22c37e86 | C     | --           | Retries ACP initialization on a v2 decode failure; local has no v2 decode path, only the v1 connection.                                                            |
| c671f77ffd | A     | b15acebede   | Clean cherry-pick; keyed inserts land in place instead of rebuilding the tree.                                                                                     |
| f00a94318c | C     | --           | Falls back to textures on GL without shader storage, but local `wgpu_renderer.rs` diverged by more than a thousand lines and the pick conflicted in 13 hunks.      |

Totals: two `A` (one landed, one already equivalent), two `B`, sixteen
`C`.

## Applied Work

- `c671f77ffd` used `git cherry-pick -x -s` against a clean tree and kept
  the upstream message.
- `d551a5fd50` started as a cherry-pick, conflicted in
  `RenderGlyphParams::hash`, and was committed with the port payload.
- `9d1a1bf541` started as a cherry-pick, conflicted on the local restart
  path, and was committed with the port payload.
- `78edf1df97` needed no code change; the local search path is the
  behavior upstream arrived at.

No named upstream remote, push, or pull request was created.

## Per-Commit Narrative

### 29de395fea — relative `./` and `../` search (C)

The feature itself is philosophy-safe, but it is not isolatable here.
`git cherry-pick` conflicted in
`open_path_prompt/src/open_path_prompt.rs`,
`open_path_prompt_tests.rs`, `recent_projects/src/remote_servers.rs`,
`remote_connection/src/remote_connection.rs` and
`toolchain_selector/src/toolchain_selector.rs`. Against the upstream
pre-image, local `project.rs` differs by 1031 added and 2236 removed
lines and `remote_servers.rs` by 1738 added and 1100 removed, so the
retained `relative_to` invariant would have to be rebuilt on four
rewritten files rather than ported. Aborted; nothing applied.

### d31d1074a0 — native view focus and input (C)

This is the follow-up to `f4fe0be1f8`, which batch E rejected because it
introduces `NativeView` on the absent `crates/gpui_apple`. The commit
edits `crates/gpui/src/window/native_view.rs` and
`crates/gpui_macos/src/window/native_views.rs`; neither path exists in
this tree, and it depends on a commit classified `C`.

### f126950f9d — two clients on one remote server (C)

The production change is a `ActiveRemoteClients` global that supersedes a
reconnecting client, but the fix arrives with tests written against mock
transport fields local no longer has: `crates/remote/src/transport/mock.rs`
kept only `options`, `server_channel`, `server_cx` and `path_style`, so
`proxy_tx`, `reported_options` and the new `proxy_exit_code` have no
host, and `recent_projects/src/remote_connections.rs` restructured its
existing-workspace branch into `workspace::WorkspaceOpenTarget` where the
patch expects a plain record. The pick conflicted in all three files.

### 9935ef714a — desktop text scaling on Linux (C)

The portal read, the `App::text_scale_factor` accessor and
`App::on_text_scale_factor_change` describe a setting no ZZZ surface
consumes, and upstream says the Zed-side use is a follow-up, so the
isolation test fails on "a real ZZZ caller exists". The pick would also
have to be reshaped twice: local `App` stores
`SubscriberSet<(), Handler>` and calls `retain(&(), …)` where upstream
uses `SingleSubscriberSet<Handler>`.

### 11c07191f2 — iOS keyboard occlusion (C)

The commit rewrites `crates/gpui_web/src/viewport.rs`, which this tree
deleted; `git cherry-pick` reports the path as deleted by us. The
behavior it fixes is iOS-only and ZZZ carries no `gpui_ios`.

### d3bba51492 — ACP SDK 5.5.1 and schema 1.21.2 (C)

`Cargo.toml` already pins `agent-client-protocol = "=5.5.1"` locally, so
the dependency bump has no work left. The 837-line body aligns the schema
inside `crates/agent_servers/src/acp/v1.rs` and `acp/v2.rs`, which do not
exist here: local `crates/agent_servers/src/acp.rs` is one module with no
`v2::` reference at all. Restoring that split is the rejected v2
architecture, not a portable hunk.

### 2fc98e1e9b and ac22c37e86 — ACP v2 descriptions and v1 retry (C)

Both are expressed against the v2 connection: `2fc98e1e9b` renders v2
option descriptions as Markdown through `markdown` and `config_options`,
and `ac22c37e86` reclassifies a v2 initialize decode failure so the
connection retries v1. Local connects through the v1 path only, so
neither commit has a decode path, a description field, or a caller to
change.

### d551a5fd50 — signed zero hashing (B)

Retained: `Pixels`, `RenderGlyphParams`, the line-layout run hash and
`DocumentColor` all hash through `gpui_util::canonical_float_bits`, so
`-0.0` and `0.0` agree with the hand-written `PartialEq` impls. The new
`crates/gpui/tests/eq_hash.rs` covers the first two and a `project` unit
test covers `DocumentColor`.

Omitted: nothing. The one conflict was `RenderGlyphParams::hash`, where
the local struct carries `synthetic_italic` and `synthetic_bold`; the
port hashes both after the canonical scale factor instead of upstream's
scale-factor-only body. The new test initializes those two fields locally,
which upstream's pre-image does not have.

### 9bf75d5434 and d5b6a7f1ec — git graph selection (C)

ZZZ moved the graph out of `git_ui` into `crates/git_graph`, so both
patches had to be re-pointed before they could even be checked. Applying
them to the local path conflicts in three and two hunks respectively:
`load_selected_commit_message` and the commit-diff task have a different
shape locally, and `d5b6a7f1ec` needs `ContextMenu::target_entry_index`
plus a focusable table state, while the local menu still exposes
`entry_idx` and the `ContextMenuSubmenu` rewrite that introduces the
index was rejected in the 2026-11-04 syncs.

### b77f61f3e3 — remove unused icons (C)

Four of the eight removed variants are live locally: `IconName::Attach`
in `telegram_ui/src/telegram_panel.rs`, `IconName::BookCopy` in
`rules_library/src/rules_library.rs`, `IconName::DatabaseZap` in six
`database_ui` sites, and `IconName::TextSnippet` in
`audio_viewer/src/ui.rs`. Only `Indicator`, `ToolCopy`,
`ToolNotification` and `TriangleRight` are unused here, so the patch
cannot land whole and the remainder is asset-only dead-code removal with
no behavior to check.

### 9d1a1bf541 — initializeCommand on restart (B)

Retained: `check_for_existing_devcontainer` runs
`run_initialize_commands` before it starts a stopped container, so a
restart follows the devcontainer lifecycle spec instead of skipping the
command. The test asserts `touch` was recorded and the restart succeeded.

Omitted: upstream's compose-resume branch and its re-inspect after the
start; the local restart path starts the plain container and already
inspects once. Local `FakeDocker` starts containers rather than failing
every start, so the test asserts the recorded command and a successful
restart instead of `DockerNotAvailable`.

### 78edf1df97 — search threads by displayed title (A, already equivalent)

Local `ThreadMetadata` has no `title_override` field, so
`ThreadMetadata::display_title` already resolves to the stored title or
`DEFAULT_THREAD_TITLE`, and
`threads_archive_view::title_matches_query` already fuzzy-matches that
same value. Upstream's bug needs the user-set-title override it removed,
which this tree never carried; no code change was made.

### f00a94318c — texture fallback on GL without shader storage (C)

Local `crates/gpui_wgpu/src/wgpu_renderer.rs` is 1632 lines against the
2889-line pre-image this patch edits, and the pick conflicted in eleven
hunks there and two in `wgpu_context.rs`. The feature spans the pipeline
layout, bind groups, the subpixel shader and instance-data plumbing, so
it cannot be isolated from the divergence that already exists in the
local renderer.

## Verification

| Check                                                                                           | Result  | Notes                                                                                                                          |
| ----------------------------------------------------------------------------------------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `cargo check --locked -p gpui -p project -p gpui_util`                                          | PASS    | After the `d551a5fd50` port.                                                                                                   |
| `cargo test --locked -p gpui --test eq_hash`                                                    | PASS    | 2 passed; the first run failed to build because the local struct has the two synthetic fields, which the ported test now sets. |
| `cargo test --locked -p dev_container test_runs_initialize_command_before_restarting_container` | PASS    | 1 passed after adapting the assertion to the local `FakeDocker`.                                                               |
| `cargo test --locked -p sum_tree`                                                               | PASS    | 11 passed, including the new `test_inserts_in_place`.                                                                          |
| `cargo check --locked --workspace`                                                              | PASS    | Warnings only, all pre-existing manifest warnings.                                                                             |
| `cargo fmt --all -- --check`                                                                    | FAIL    | Pre-existing: `crates/csv_preview/src/parser.rs:698` is unformatted on `main` at `e3ca693341` and untouched by this batch.     |
| `git diff --check`                                                                              | PASS    |                                                                                                                                |
| `./script/backfill-upstream-ledger` and `cargo xtask check upstream-ledger`                     | PASS    | Ledger regenerated from this report's decision table.                                                                          |
| Prettier over this report                                                                       | PASS    | `3.9.10`, the version pinned by `script/prettier`.                                                                             |
| macOS, Windows and iOS runtime behavior                                                         | NOT RUN | This host is Linux. The native-view, `gpui_web` and wgpu hunks were only inspected.                                            |
