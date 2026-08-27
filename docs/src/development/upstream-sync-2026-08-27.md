---
title: Upstream Sync 2026-08-27
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-08-27

## Scope

- Target branch: `sync/upstream-2026-08-27` from `main` at
  `2761d2445eca441bc5948e8d35bebb01274496cf`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Reviewed upstream head: `4c7244790a075e862eeb4e5ccc12d6c8f5da6f7e`
- Live upstream head queried: `8166e3d7b8b42d8aaf4d4dee7fcd25ab4ec65105`
- Query time: `2026-08-27T23:25:05+02:00`
- Requested range starts after `aa3718614b3ade75524be6f8b2e101bd1166e02c`

`A` is a complete safe absorption or an already-equivalent local change.
`B` needs a local equivalent port or further API review and is deliberately not
claimed as synchronized unless a local commit is listed. `C` is rejected by
ZZZ's local-first, no-account, ACP-only boundary, or failed isolation.

The reviewed baseline is now `4c7244790a075e862eeb4e5ccc12d6c8f5da6f7e`.
Default batch was the first 20 commits after `aa3718614b`. 155 upstream commits
remain after this head. Counts: 3 A, 6 B, 11 C.

## Decisions

| Upstream | Class | Local commit | Disposition                                                     |
| -------- | ----- | ------------ | --------------------------------------------------------------- |
| 83f3a8e3 | C     | --           | ChatGPT Subscription Responses transport; `HttpSend` absent.    |
| 78712609 | A     | 3510b24e     | Cherry-picked with `-x -s`.                                     |
| 4bdf188c | C     | --           | Stash tracked/staged options; git_panel and git.proto conflict. |
| 2893b86b | B     | dcf64063     | macOS simple fullscreen covering the notch.                     |
| 1274a5dc | B     | 4d7ead54     | VS Code npm task `path` property.                               |
| cf08569e | B     | 62714f97     | `file_scan_exclusions` `"..."` splice.                          |
| fd5cd939 | B     | 5c0060f4     | Markdown loose-list task markers.                               |
| fdad9186 | C     | --           | `git_ui_core` askpass files are absent.                         |
| 05473ed8 | C     | --           | `csv_preview` rename; local crate already diverged.             |
| dbc90d18 | C     | --           | Depends on rejected `05473ed8` crate rename.                    |
| a7d74150 | C     | --           | Settings UI Default/Custom needs absent `PixelSetting`.         |
| 03c9c4e7 | C     | --           | `ParsedSvg` / `render_parsed` APIs are absent.                  |
| 87324045 | C     | --           | Lockfile-only `async-tar` fork for Cursor ACP download.         |
| 3624a5bf | C     | --           | Depends on rejected `6dee3fc7` diagnostic proto.                |
| 35f63e40 | B     | 0322d76b     | Markdown code-block `buffer_line_height`.                       |
| 2040e0de | C     | --           | `spawn_dedicated` / scheduler rewrite plus lockfile churn.      |
| 00c0e96e | C     | --           | `LoadedFile` Rope rewrite conflicts on local worktree decode.   |
| aad75630 | A     | 1baf13a1     | Cherry-picked with `-x -s`.                                     |
| 3f660a0a | B     | 3abceceb     | Drop deprecated `std::usize` / `std::u32` / `std::u64` imports. |
| 4c724479 | A     | 17ec7f5a     | Cherry-picked with `-x -s` after `aad75630`.                    |

## Applied Work

The work branch contains the listed A/B local commits. Every direct upstream
commit was created with `git cherry-pick -x -s`; B commits retain their full
`Upstream:` trailer and explain omissions. No remote branch, pull request, or
upstream remote was created.

## Per-commit notes

### B ports

- `2893b86b`: `fullscreen_mode`, GPUI simple-fullscreen APIs, macOS
  borderless implementation, title-bar padding, and `ToggleFullScreen`
  routing. Omitted agent_ui, sidebar, settings_ui page metadata, and
  `all-settings.md`.
- `1274a5dc`: npm `path` as worktree-relative cwd, with `options.cwd`
  still winning. Omitted gulp/shell test-file split; kept
  `typescript.json`.
- `cf08569e`: `SplicingVec` so `"..."` extends inherited
  `file_scan_exclusions`. Omitted agent-tool production hunks and docs.
  Test assignments adapted to `SplicingVec::from`.
- `fd5cd939`: look through a wrapping paragraph for task markers. Kept
  local `ToggleState::Selected` / `Unselected`.
- `35f63e40`: fenced code blocks use relative `buffer_line_height`.
  Omitted render tests that need diverged preview helpers.
- `3f660a0a`: dropped the deprecated integer prelude imports on the
  local files that still had them.

### Representative C

Philosophy: ChatGPT Subscription Responses transport, Cursor ACP tar
fork.

Missing architecture: `RequestError::HttpSend` / `compact_response`,
`git_ui_core`, `ParsedSvg`, `PixelSetting`, `spawn_dedicated`.

Unisolatable conflicts: git_panel + git.proto stash UI, csv_preview
rename, diagnostic related-info proto from `6dee3fc7`, worktree
`LoadedFile` Rope rewrite on a file that already has a local 6GB
workaround for the same peak-memory issue.

## Verification

```text
PASS git merge-base --is-ancestor aa3718614b FETCH_HEAD
PASS cargo check --locked -p anthropic -p language_models
PASS cargo test --locked -p anthropic --lib list_models_preserves_anthropic_api_errors
PASS cargo test --locked -p task -- can_deserialize_npm_tasks
PASS cargo test --locked -p settings_content --lib test_file_scan_exclusions
PASS cargo test --locked -p markdown --lib test_task_list_marker_for_item
PASS cargo check --locked -p settings_content -p settings -p worktree
PASS cargo check --locked -p gpui -p workspace -p markdown -p task
PASS cargo check --locked -p gpui_macos -p platform_title_bar
PASS cargo check --locked --tests -p zzz
PASS cargo check --locked --tests -p worktree -p agent -p project_panel -p anthropic
NOT RUN macOS / Windows / wasm32 gpui_web runtime
NOT RUN cargo test --workspace
NOT RUN cargo fmt --check (edition-2024 let-chain rustfmt errors on this host)
```

The reviewed baseline is `4c7244790a075e862eeb4e5ccc12d6c8f5da6f7e`.
Work remains on `sync/upstream-2026-08-27` and has not been merged to
`main`.

## Continuation: 2026-08-28

### Scope

- Target branch: `sync/upstream-2026-08-27`, continuing from `main` at
  `2761d2445eca441bc5948e8d35bebb01274496cf`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline:
  `4c7244790a075e862eeb4e5ccc12d6c8f5da6f7e`
- Reviewed upstream head: `582e6a5789570f9abf9eab17bff027eaf18a0e3c`
- Live upstream head queried: `8166e3d7b8b42d8aaf4d4dee7fcd25ab4ec65105`
- Query time: `2026-08-28T00:29:32+02:00`

This continuation reviewed the next 20 commits after `4c724479`. Counts:
4 A, 9 B, 7 C. The reviewed baseline is now
`582e6a5789570f9abf9eab17bff027eaf18a0e3c`; 135 commits remain through the
queried live head.

### Decisions

| Upstream | Class | Local commit | Disposition                                                             |
| -------- | ----- | ------------ | ----------------------------------------------------------------------- |
| fa852694 | B     | 4b90cbaa     | Enable the existing CSV preview without an upstream feature flag.       |
| 7a7c3e1d | C     | --           | Requires the removed auto-update downloader.                            |
| 1a332533 | A     | 4bc1f8df     | Cherry-picked with `-x -s`.                                             |
| 28c0f4ae | B     | 01bf9f79     | Collapse the nearest Git tree parent.                                   |
| 99f4c21c | C     | --           | OpenCode Go/Zen subscription-model catalog and settings.                |
| c43e2d97 | B     | 2a9c84a0     | Reject failed XKB context initialization.                               |
| 45ae0572 | B     | 1450b072     | Stream web Fetch responses.                                             |
| d70c45e5 | C     | --           | Needs absent web clipboard and external-drag GPUI APIs.                 |
| fa00dccc | C     | --           | Large `crates/path` migration conflicts with ZZZ `paths`.               |
| 9bb47879 | B     | 79a0a31d     | Hide Markdown syntax that does not render from find matches.            |
| 0f84a49e | C     | --           | Native cloud websocket belongs to rejected account/collaboration paths. |
| 71507659 | B     | ffbd393f     | Preserve `--user-data-dir` on normal restart.                           |
| 242fe31a | A     | fe810e97     | Cherry-picked with `-x -s`.                                             |
| f1cdbaad | B     | c9eded99     | Disable invalid Git-panel discard action.                               |
| 7b48fc68 | A     | 6623fd1d     | Cherry-picked with `-x -s`.                                             |
| 82854434 | B     | 8263bdc4     | Preserve lookaround context during regex replacement.                   |
| 0cfb1ca1 | B     | c14c6130     | Normalize Pyright and basedpyright analysis settings.                   |
| 0a4a4a95 | C     | --           | Upstream release-version and lockfile metadata only.                    |
| badd2157 | A     | 7e7bf33b     | Cherry-picked with `-x -s`.                                             |
| 582e6a57 | C     | --           | Broad async language-loader and query API rewrite.                      |

### Applied work

Direct A commits `1a332533`, `242fe31a`, `7b48fc68`, and `badd2157` were each
absorbed with `git cherry-pick -x -s` as their listed local commits. The B
ports below have `Upstream`, `Retained`, and `Omitted` trailers in their local
commits.

- `fa852694`: enabled ZZZ's existing `csv_preview` surface without moving to
  upstream's renamed `tabular_data_preview` crate.
- `28c0f4ae`: added nearest-parent collapse and Vim/Helix tree navigation;
  omitted the incompatible visual-test helper.
- `c43e2d97`: reject null XKB contexts in existing X11 and Wayland paths.
- `45ae0572`: stream browser Fetch data with backpressure and cancellation.
- `9bb47879`: omit non-rendered Markdown syntax from preview search results.
- `71507659`: retain the canonical `--user-data-dir` across Linux, macOS, and
  Windows restarts; omit deleted updater-only paths.
- `f1cdbaad`: enable “Discard Tracked Changes” only with staged tracked files.
  Omit upstream directory-scoped discard because ZZZ's context-menu state does
  not retain a target entry.
- `82854434`: calculate same-line replacement captures from their source
  context, retaining lookahead and lookbehind behavior. Omit the diverged
  multibuffer fixture.
- `0cfb1ca1`: expose analysis settings in both nested and legacy dotted forms,
  merging values without loss. Omit unrelated toolchain-default changes and
  the documentation rewrite.

### Rejected work

- `7a7c3e1d` needs Zed's auto-update download state. Its generic GPUI
  system-wake subscription is already present locally, but the requested
  restart behavior has no allowed updater caller.
- `99f4c21c` configures subscription-bound OpenCode Go and Zen model catalogs
  and removes a subscription tier, outside ZZZ's silent manual-provider
  boundary.
- `d70c45e5` introduces the unabsorbed web async-clipboard and external-drag
  GPUI surface; no complete ZZZ caller exists for it.
- `fa00dccc` migrates path behavior into the absent `crates/path` crate and is
  not isolatable from its broader Windows remote-path rewrite.
- `0f84a49e` optimizes an account/collaboration cloud websocket route, which
  ZZZ does not retain as a product surface.
- `0a4a4a95` is upstream release metadata with no independent ZZZ behavior.
- `582e6a57` adds public async language-loader and query-selection APIs across
  extension and grammar loading. It is an unisolatable architecture rewrite,
  not a current ZZZ caller fix.

### Verification

```text
PASS git merge-base --is-ancestor 4c724479 FETCH_HEAD
PASS cargo check --locked -p git_ui -p editor -p project -p languages -p search
PASS cargo test --locked -p search test_replace_with_lookaround
PASS cargo test --locked -p languages test_normalize_
PASS cargo test --locked -p git_ui test_discard_tracked_changes_respects_staging
PASS git diff --check
FAIL cargo fmt --all --check (pre-existing formatting drift outside this batch)
NOT RUN macOS / Windows / wasm32 gpui_web runtime
NOT RUN cargo test --workspace
```

The reviewed baseline is `582e6a5789570f9abf9eab17bff027eaf18a0e3c`.
Work remains on `sync/upstream-2026-08-27` and has not been merged to
`main`.

## Continuation: 2026-08-28 (second batch)

### Scope

- Target branch: `sync/upstream-2026-08-27` from `main` at
  `2761d2445eca441bc5948e8d35bebb01274496cf`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline:
  `582e6a5789570f9abf9eab17bff027eaf18a0e3c`
- Reviewed upstream head:
  `09adbb01f6ed625a976339f014d6c11690aa6301`
- Live upstream head queried:
  `8166e3d7b8b42d8aaf4d4dee7fcd25ab4ec65105`
- Query time: `2026-08-28T01:19:58+02:00`

This continuation reviewed the next 20 commits after `582e6a57`. Counts:
4 A, 10 B, 6 C. The reviewed baseline is now
`09adbb01f6ed625a976339f014d6c11690aa6301`; 115 commits remain through the
queried live head.

### Decisions

| Upstream | Class | Local commit | Disposition                                                   |
| -------- | ----- | ------------ | ------------------------------------------------------------- |
| d5dc01f2 | B     | 06ab03c7     | Localized Windows ZZZ registry-key lookup.                    |
| 314e0902 | B     | 798e7cba     | Bash language-server workspace settings.                      |
| 2936989f | C     | --           | New GPUI `LineLayout` APIs have no ZZZ caller.                |
| 1861e58f | C     | --           | Telemetry/hang journaling violates the no-telemetry boundary. |
| 8bbbeb3d | A     | fd98f700     | Cherry-picked with `-x -s`.                                   |
| e3056061 | B     | 3548cd29     | Render C0 control characters in existing labels.              |
| 30aea6ac | B     | b0b6e591     | Add `in_preview` keybinding context.                          |
| 6a37cc11 | A     | 2a60883c     | Cherry-picked with `-x -s`.                                   |
| 2bf9e264 | B     | 2e6f8124     | Support terminal Ctrl-Alt ASCII keys.                         |
| 6e0a0835 | C     | --           | CI-only `gh` toolchain acquisition.                           |
| 32a0e813 | B     | 94e27269     | Align debugger step bindings on local contexts.               |
| a58fff13 | A     | e7d3e123     | Cherry-picked with `-x -s`.                                   |
| cef06d35 | C     | --           | Depends on the rejected worktree streaming rewrite.           |
| 4d1935b8 | A     | 2ede3ec0     | Cherry-picked with `-x -s`.                                   |
| 282f47a5 | C     | --           | cargo-shear, lockfile, and CI cleanup only.                   |
| 1b04e4ca | B     | 4d90d67e     | Do not bundle GLib in Linux archives.                         |
| dbdcb310 | C     | --           | Requires the absent `lsp_locations` crate.                    |
| 3ea4d186 | B     | 49ea83c5     | Canonicalize case-insensitive LSP paths.                      |
| 58006060 | B     | 66b992dd     | Remove rename-created directories on undo.                    |
| 09adbb01 | B     | 2b597735     | Persist recent navigation history across sessions.            |

### Applied work

Direct A commits `8bbbeb3d`, `6a37cc11`, `a58fff13`, and `4d1935b8` were
absorbed with `git cherry-pick -x -s` as their listed local commits. The B
ports have `Upstream`, `Retained`, and `Omitted` trailers in their local
commits.

- `d5dc01f2`: read the installer-written ZZZ registry keys while retaining
  ZZZ's fallback title and local feature gates.
- `314e0902`: read bash-language-server workspace configuration from ZZZ's
  existing LSP settings store.
- `e3056061`: replace C0 controls in labels, tabs, file-finder paths, picker
  matches, terminal titles, and CSV headers; omit unavailable git UI files.
- `30aea6ac`: expose preview-item state as the editor `in_preview` context;
  omit a duplicate delimiter-expansion test.
- `2bf9e264`: emit ESC-prefixed control bytes for Ctrl-Alt letters and fix the
  modified F5 lookup.
- `32a0e813`: add VS Code-style debugger step bindings while preserving ZZZ's
  ACP/global keymap layout.
- `1b04e4ca`: exclude GLib/private dependencies from Linux bundles and clarify
  PipeWire errors, adapted to ZZZ's packaging names.
- `3ea4d186`: canonicalize LSP-opened paths before worktree lookup and add
  case-insensitive FakeFs coverage.
- `58006060`: track missing parent directories as undoable create/remove
  operations around project-panel renames.
- `09adbb01`: persist and restore bounded recent project paths, update them on
  rename, and clear/save them through existing workspace serialization. The
  active-path test was adapted to invoke ZZZ's local callback directly.

### Rejected work

- `2936989f` adds public `LineLayout` split/paint APIs without a current ZZZ
  caller, so importing them would be unused scaffolding.
- `1861e58f` journals foreground work and reports hang incidents through
  telemetry, contrary to ZZZ's local-first no-telemetry policy.
- `6e0a0835` changes CI acquisition of `ts_query_ls` and has no product
  behavior to absorb.
- `cef06d35` depends on the earlier rejected `00c0e96e` large-file Rope
  rewrite and cannot be isolated from it.
- `282f47a5` only switches dependency-analysis tooling and lockfile/CI data.
- `dbdcb310` routes locations through the absent upstream `lsp_locations`
  crate, with no complete ZZZ equivalent.

### Verification

```text
PASS git merge-base --is-ancestor 582e6a57 FETCH_HEAD
PASS cargo check --locked -p workspace -p file_finder -p project -p editor
PASS cargo test --locked -p workspace navigation_history
PASS cargo test --locked -p workspace test_active_project_path_changes_are_persisted
PASS git diff --check
FAIL cargo fmt --all --check (pre-existing formatting drift outside this batch)
NOT RUN macOS / Windows / wasm32 gpui_web runtime
NOT RUN cargo test --workspace
```

The reviewed baseline is `09adbb01f6ed625a976339f014d6c11690aa6301`.
Work remains on `sync/upstream-2026-08-27` and has not been merged to
`main`.
