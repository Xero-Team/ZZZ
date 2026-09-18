---
title: Upstream Sync 2026-09-18
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-09-18

## Scope

- Target branch: `sync/upstream-2026-09-18` from `sync/upstream-2026-09-17`
  (originally `main` at `e6adb70968552e53dae959f107df4f8ac03470d9`)
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previously reviewed baseline: `490aad88d5c754e4b0fbbb2bf1e3d6936df27729`
- Reviewed upstream head: `d7f28899166cead457a2239ad23056d0a980bd7c`
- Live upstream head queried: `650a8d1bedaca3f841f1c11f89fa786f574c2aab`
- Query time: `2026-09-18T19:58:57+02:00`
- Reviewed range: `490aad88..d7f28899`

This batch covered the first 20 commits after the previous baseline. Counts:
5 A, 7 B, and 8 C. The reviewed baseline is now
`d7f28899166cead457a2239ad23056d0a980bd7c`; 143 commits remain through the
queried live head.

## Decisions

| Upstream | Class | Local commit       | Disposition                                                                                      |
| -------- | ----- | ------------------ | ------------------------------------------------------------------------------------------------ |
| 3384317a | C     | --                 | `util/debug-embed` / `fs_embed` are absent; enabling the feature would not compile.              |
| 6f72bdb7 | B     | 2ae663c6           | ACP thread copies plain text by default; keep i18n labels.                                       |
| e2534d23 | A     | 6059ecd9           | Cherry-picked with `-x -s`.                                                                      |
| 20d3cd1d | B     | c533f5c7, f15db659 | Join-line prefix fix; drop rust block-comment assertions that need comment overrides.            |
| ff6a6abb | A     | 8fe587d9           | Cherry-picked with `-x -s`.                                                                      |
| 10676bad | C     | --                 | Collab peer search sharing of `private_files`.                                                   |
| f29c8eaf | A     | 6abea349           | Cherry-picked with `-x -s`.                                                                      |
| 4612aa2f | C     | --                 | Collab selection broadcast skip needs `Project::is_shared` and deleted `editor/src/input.rs`.    |
| 72b02bf1 | A     | b1e2ffc9           | Cherry-picked with `-x -s`.                                                                      |
| f83313d0 | A     | e7f57596           | Cherry-picked with `-x -s`.                                                                      |
| 58962741 | B     | 675a0796           | Owning GPUI asset cache; omit wasm lock/channel Cargo.toml splits.                               |
| 71b60bba | C     | --                 | Upstream release metadata version bump.                                                          |
| fceace0b | B     | 60f16bbe           | Emmet language-based suggestion on existing notifications; omit zed.dev URL and lockfile tests.  |
| 0690433b | B     | 61330ebc           | LSP executeCommand and showDocument; omit collab, telemetry proto IDs, and `lsp_locations`.      |
| 002161d5 | B     | b0806d4a           | Inlay hint commands on `element.rs`; omit deleted `element/mouse.rs`.                            |
| 33c6212b | C     | --                 | macOS Space restore rewrites persistence (`set_session_id`) and MultiWorkspace restore.          |
| 45077524 | C     | --                 | Idle-sleep API spans every GPUI backend, native-agent settings UI, and deleted `livekit_client`. |
| 907b55f7 | B     | cc07c65f           | macOS `register_url_scheme` on objc2; enable `NSWorkspace` and `block2` features.                |
| 318c664e | C     | --                 | Markdown parse-time highlight cache rewrites already-diverged `markdown.rs` (10 conflicts).      |
| d7f28899 | C     | --                 | Buffer highlight cache depends on rejected `318c664e` `ResolvedHighlights`.                      |

## Applied work

Direct A commits `e2534d23`, `ff6a6abb`, `f29c8eaf`, `72b02bf1`, and
`f83313d0` were absorbed with `git cherry-pick -x -s`.

B ports retain `Upstream`, `Retained`, and `Omitted` trailers:

- `6f72bdb7`: ACP thread context menu copies selected text as plain text and
  offers copy as markdown. AgentPanel markdown keymaps no longer override
  ctrl/cmd-c.
- `20d3cd1d`: join lines skip block-comment prefixes unless the language scope
  override is `comment`, so markdown `*bar*` is kept. The rust `/* */`
  assertions were dropped.
- `58962741`: GPUI asset loads use owning `CachedLoad` entries.
  `fetch_asset` returns `Option`.
- `fceace0b`: HTML-like buffers suggest the Emmet extension after language
  detection.
- `0690433b`: language servers can execute commands and show documents,
  including a command selector and remote-server forwarding.
- `002161d5`: inlay hint label parts can carry and activate LSP commands.
- `907b55f7`: `MacPlatform::register_url_scheme` uses objc2 `NSWorkspace`.

## Per-commit notes

### 3384317a

Adds `util/debug-embed` to `remote_server`'s `debug-embed` feature. ZZZ's
`util` crate has no such feature and no `fs_embed!` panic path. The earlier
`debug-embed` commits are already behind the reviewed baseline and were not
absorbed.

### 10676bad, 4612aa2f

Collab-only. Private-file search sharing is a peer RPC check.
`should_broadcast_selections` needs `Project::is_shared()`, which is absent,
and the commit also touches deleted `editor/src/input.rs`.

### 71b60bba

Bumps the Zed crate from 1.20.0 to 1.21.0. Upstream release metadata.

### 33c6212b

Adds GPUI `native_window_state` and a workspace DB column, then restores
windows through MultiWorkspace. Auto-merge deleted `set_session_id` and
conflicted on ZZZ's `DetachFromSession` serialize path.

### 45077524

New `Platform::prevent_idle_sleep` plus native-agent settings, ACP thread
rewrites, settings UI, and livekit. `livekit_client` is deleted locally.
A GPUI API with no current ZZZ caller would also be C.

### 318c664e, d7f28899

Parse-time markdown highlight caching rewrites `crates/markdown/src/markdown.rs`
in ten conflict hunks. The follow-up buffer row-chunk cache needs
`ResolvedHighlights` from that commit.

## Verification

```text
PASS git merge-base --is-ancestor 490aad88 FETCH_HEAD
PASS cargo check --locked -p gpui -p editor -p language -p language_core
PASS cargo check --locked -p extensions_ui -p lsp_command_selector -p project
PASS cargo check --locked -p gpui_macos
PASS cargo test --locked -p editor --lib test_join_lines_strips_comment_prefix
PASS cargo test --locked -p editor --lib test_rotate_selections
PASS cargo test --locked -p language --lib test_injection_grouped_by_host
PASS git diff --check
BLOCKED cargo test --locked -p tab_switcher --lib (baseline missing i18n::GlobalI18nService)
NOT RUN macOS / Windows / wasm32 runtime tests
NOT RUN cargo test --workspace
NOT RUN cargo fmt --check (known edition-2024 formatting drift on this host)
```

The reviewed baseline is `d7f28899166cead457a2239ad23056d0a980bd7c`.
Work remains on `sync/upstream-2026-09-18` and has not been merged to `main`.
