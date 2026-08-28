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

## Continuation: 2026-08-28 (ninth batch)

### Scope

- Target branch: `sync/upstream-2026-08-27` from `main` at
  `2761d2445eca441bc5948e8d35bebb01274496cf`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline:
  `01acd0ee8e906dd0ec8b526fe08da94444a5e2af`
- Reviewed upstream head:
  `669bede5c582140d2f3917e2c5fae5b764f9023f`
- Live upstream head queried: `669bede5c582140d2f3917e2c5fae5b764f9023f`
- Query time: `2026-08-28T15:48:00+02:00`

This continuation reviewed the seven commits after `01acd0ee`. Counts:
0 A, 7 B, 0 C. The reviewed baseline is now
`669bede5c582140d2f3917e2c5fae5b764f9023f`; no commits remain through the
queried live head.

### Decisions

| Upstream | Class | Local commit     | Disposition                                                               |
| -------- | ----- | ---------------- | ------------------------------------------------------------------------- |
| a6ffd721 | B     | 2cae688b         | Delay ETW output-path selection until save.                               |
| c90f1fed | B     | 6d486563         | Preserve platform-specific keybinding labels in Which Key.                |
| 4b3ef3e4 | B     | 089592b          | Parse partial-clone filter metadata in remote URLs.                       |
| cf1900f4 | B     | 47fb250          | Refresh YARA naming, links, and setup documentation.                      |
| 351899e5 | B     | e011fcf, a096808 | Reveal already-open files in another pane when configured.                |
| 8ee36b68 | B     | df35f2f          | Anchor bare-repository worktree names and paths to repository identity.   |
| 669bede5 | B     | f1d34c2          | Detect languages in eligible untitled buffers with confidence thresholds. |

### Applied work

All seven upstream commits conflicted with ZZZ's branded documentation,
deleted modules, renamed crates, or existing local APIs, so each was ported
as an isolatable B change with `git commit -s`. The `351899e5` adaptation uses
two local commits because one remaining editor callsite required a follow-up
signature fix. No upstream remote, branch, pull request, or release metadata
was created.

- `a6ffd721`: retained ETW session state and deferred output-path selection;
  adapted Windows documentation to ZZZ and omitted no product behavior.
- `c90f1fed`: retained `KeybindingKeystroke` metadata through filtering,
  grouping, sorting, and display while preserving local grouping tests.
- `4b3ef3e4`: accepted `(fetch) [filter]` remote lines without changing ZZZ's
  repository index implementation.
- `cf1900f4`: applied YARA capitalization, corrected the language index link,
  and added YLS setup while preserving ZZZ branding and issue links.
- `351899e5`: added the `reveal_if_open` setting and cross-pane reuse for path
  and project-item opens; omitted the new uncalled `open_url_or_file` API and
  deleted upstream modules.
- `8ee36b68`: adapted bare-repository identity anchoring to ZZZ's `git_ui`
  crate and path-style safeguards; omitted the upstream `.rules` edit.
- `669bede5`: added the `language_detection` crate, confidence-gap model
  selection, untitled-buffer lifecycle hooks, persistence, settings, and
  tests; adapted language loading and editor serialization to ZZZ APIs.

### Verification

```text
PASS git merge-base --is-ancestor 01acd0ee FETCH_HEAD
PASS cargo check --locked -p git -p project -p git_ui -p workspace -p editor -p settings -p settings_ui
PASS cargo check --locked -p language_detection -p editor -p project_symbols -p project -p settings_ui -p zzz
PASS git diff --check
PASS cargo check --locked
PASS ./script/clippy
NOT RUN macOS / Windows / wasm32 gpui_web runtime
NOT RUN cargo test --workspace
```

Additional cleanup landed as `1824b5fdc8` (test-context, type-import, and
dead-code fixes) and `ef1a39950b` (formatting for the adapted worktree picker
import), both required to keep the full workspace checks clean.

The reviewed baseline is `669bede5c582140d2f3917e2c5fae5b764f9023f`.
Work remains on `sync/upstream-2026-08-27` and has not been merged to `main`.

## Continuation: 2026-08-28 (fifth batch)

Reviewed upstream `35aab214..391a66a5`; live head `cf1900f44d30c771207e36e2c9094b6c1f659bea`.
Previous baseline: `7f2a2c3c3ee2f23f28772dee7661fb98d3910990`.

| Upstream | Class | Local commit       | Disposition                                                      |
| -------- | ----- | ------------------ | ---------------------------------------------------------------- |
| 35aab214 | B     | adef72b4           | Document Tailwind CSS IntelliSense for CSS files.                |
| 6805d952 | A     | 870a48d7           | Make slang-server the default SystemVerilog LSP.                 |
| 93f07f6d | B     | ca0d018c           | Preserve modal focus when terminals appear.                      |
| d6449a9e | B     | df156a42           | Bound oversized LSP hover content.                               |
| 1b86941c | B     | dfedc287           | Add Gemini 3.5 Flash-Lite metadata.                              |
| 4c4b19a2 | B     | 57a5358b           | Retire deprecated Gemini models and aliases.                     |
| b9d1fe59 | A     | 715e321b           | Document Elixir debug adapter support.                           |
| 99b0ed6b | B     | 23e55f0b           | Preserve configured workspace session state on close.            |
| fdfd00e4 | C     | --                 | GitHub Enterprise Copilot cloud-account routing.                 |
| 6e2fae61 | C     | --                 | Wasmtime/lockfile-only dependency update.                        |
| a170a124 | A     | 2da28878           | Fix fold-at-level function-body boundaries.                      |
| 49a841c7 | A     | 606fb782           | Pass task-template environment to PythonLocator.                 |
| f1d27d54 | B     | c7d1e3f4, dab7f655 | Prewarm Linux font-match caches on local cosmic-text paths.      |
| 5a2039b2 | C     | --                 | Requires absent ProviderRejection APIs and native-agent changes. |
| eb96feb8 | A     | eab14afc           | Repaint editor gutter when bookmarks change.                     |
| 0b1bf8dc | A     | 0b7b9f2a           | Remove stray SQL statement debug output.                         |
| ff7b061d | C     | --                 | macOS provisioning-profile release metadata only.                |
| 1747596a | C     | --                 | Extension-card layout targets a replaced local component.        |
| 507a1b99 | B     | 294466a7           | Skip non-selectable entries when selecting Git changes.          |
| 391a66a5 | B     | 4a44e54d           | Split debugger continue actions on local DAP APIs.               |

### Applied work

Direct A commits `6805d952`, `b9d1fe59`, `a170a124`, `49a841c7`, `eb96feb8`,
and `0b1bf8dc` were absorbed with `git cherry-pick -x -s`. The B ports have
`Upstream`, `Retained`, and `Omitted` trailers. `f1d27d54` first added the
shared GPUI/platform and Linux startup hooks, then `dab7f655` implemented the
ZZZ cosmic-text prewarm path after the upstream fallback-chain hunk conflicted.

- `35aab214`: adapted Tailwind CSS IntelliSense guidance to ZZZ documentation.
- `93f07f6d`: guarded task and shell terminal focus with active-modal checks;
  omitted the absent serialized-restoration path.
- `d6449a9e`: added a UTF-8-safe 100,000-byte hover bound; omitted the larger
  upstream markdown fence reconstruction.
- `1b86941c` / `4c4b19a2`: added Gemini 3.5 Flash-Lite and removed deprecated
  Gemini 2.5/3.1 Flash-Lite variants while preserving ZZZ's mode API.
- `99b0ed6b`: honored `on_last_window_closed` and flushed serialization before
  removing a window, adapted to existing ZZZ persistence methods.
- `507a1b99`: added local selectable-entry and visible-index handling; omitted
  the upstream test fixture that requires absent helpers.
- `391a66a5`: added Continue Program/Continue Thread actions and DAP response
  handling; omitted the conflicting upstream DebugPanel button layout.

### Rejected work

`fdfd00e4` is data-resident GitHub Enterprise Copilot account/cloud routing,
outside ZZZ's silent manual-provider boundary. `6e2fae61` only updates
Wasmtime and lockfile metadata. `5a2039b2` depends on the absent
`ProviderRejection`/provider-category API and broad native-agent changes.
`ff7b061d` is release provisioning metadata. `1747596a` cannot be isolated
from an upstream extension-card component that ZZZ replaced with a generic
children-only card.

### Verification

```text
PASS git merge-base --is-ancestor 7f2a2c3c FETCH_HEAD
PASS cargo check --locked -p google_ai
PASS cargo check --locked -p editor
PASS cargo check --locked -p terminal_view
PASS cargo check --locked -p workspace
PASS cargo check --locked -p gpui_wgpu
PASS cargo check --locked -p zzz
PASS cargo check --locked -p debugger_ui
PASS cargo check --locked -p git_ui --tests
PASS cargo test --locked -p google_ai model_helpers_cover_built_in_aliases_and_custom_modes
PASS git diff --check
FAIL cargo fmt --all --check (pre-existing formatting drift outside this batch)
NOT RUN macOS / Windows / wasm32 gpui_web runtime
NOT RUN cargo test --workspace
```

The reviewed baseline is `391a66a5ad9f68e13a54f57b3b06f4605867614c`.
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

## Continuation: 2026-08-28 (third batch)

### Scope

- Target branch: `sync/upstream-2026-08-27` from `main` at
  `2761d2445eca441bc5948e8d35bebb01274496cf`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline:
  `09adbb01f6ed625a976339f014d6c11690aa6301`
- Reviewed upstream head:
  `ab208db8d264ad08f62bda7ba0ce560c220a347f`
- Live upstream head queried:
  `4b3ef3e45a64f30525324ef8d0d60c1adca2cd7a`
- Query time: `2026-08-28T12:04:52+02:00`

This continuation reviewed the next 20 commits after `09adbb01`. Counts:
1 A, 9 B, 10 C. The reviewed baseline is now
`ab208db8d264ad08f62bda7ba0ce560c220a347f`; 103 commits remain through the
queried live head.

### Decisions

| Upstream | Class | Local commit | Disposition                                                         |
| -------- | ----- | ------------ | ------------------------------------------------------------------- |
| c3b365d2 | C     | --           | Native Agent inline assistant is outside ZZZ's ACP-only boundary.   |
| deb194b4 | B     | de81c421     | Deduplicate overlapping LSP range-format edits.                     |
| 2b37a3ed | C     | --           | Upstream GitHub contributor-label link only.                        |
| 53b39e8e | B     | 85beb270     | Bound global gitignore matching to repository/worktree roots.       |
| b0e37a6c | B     | 2e471090     | Display Node/Python language-server script paths in LSP tooltips.   |
| f4178619 | B     | b1c5a201     | Drain buffered X11 events after foreground work.                    |
| debf6b21 | B     | df4d3558     | Order Flatpak launcher arguments before positional paths.           |
| cb1352a2 | B     | 1332e495     | Add debounce only for manual local edit-prediction providers.       |
| b427d4ec | A     | 87a8c7f3     | Cherry-picked with `-x -s`.                                         |
| 1e9f1ef4 | C     | --           | Baseten uses cloud credentials and is not a silent manual provider. |
| fe9556a1 | C     | --           | Unused browser performance-tracing API and web tracing machinery.   |
| f5e87e53 | B     | 483087bb     | Clear the existing settings search field.                           |
| 91bf967e | B     | deda284e     | Expose ZZZ's inspector as an explicit diagnostic feature.           |
| 5255bd7f | C     | --           | Needs absent `PaymentRequired` completion-error API.                |
| 84aaa525 | C     | --           | Upstream GitHub triage-project workflow and script removal.         |
| 5b70f793 | B     | ee09d831     | Use `Duration` on existing local timing paths.                      |
| ef50ad95 | C     | --           | GitHub CLA/draft pull-request cleanup automation.                   |
| eb354c8d | C     | --           | Unisolatable Wayland/GPUI render-loop architecture rewrite.         |
| 54230ad8 | C     | --           | OpenAI subscription-provider autocomplete is account-bound.         |
| ab208db8 | C     | --           | Stabilizes an upstream bracket test that ZZZ does not contain.      |

### Applied work

Direct A commit `b427d4ec` was absorbed with `git cherry-pick -x -s` as
`87a8c7f3`. The B ports have `Upstream`, `Retained`, and `Omitted` trailers in
their local commits. They cover range-format overlap handling, global
gitignore boundaries, script-path tooltips, buffered X11 events, Flatpak
launch arguments, local prediction debounce, settings search clearing,
explicit inspector enabling, and `Duration` types across existing ACP,
terminal, search, project, editor, CLI, and profiler paths.

`5b70f793` preserves millisecond configuration at the terminal settings
boundary and omits only the removed `openai_subscribed` account route.

### Rejected work

`c3b365d2`, `1e9f1ef4`, and `54230ad8` conflict with the ACP-only or silent
manual-provider boundary. `2b37a3ed`, `84aaa525`, and `ef50ad95` are upstream
contributor, triage, or CLA administration. `fe9556a1` has no ZZZ caller.

`5255bd7f` was evaluated as a manual Anthropic error-mapping port, but the
needed `LanguageModelCompletionError::PaymentRequired` variant is absent;
adding it would be unused public API, so the attempted patch was fully
reverted. `eb354c8d` requires a divergent GPUI scheduling and Wayland
presentation-state rewrite. `ab208db8` only stabilizes an upstream bracket
test and fixture absent from ZZZ.

### Verification

```text
PASS git merge-base --is-ancestor 09adbb01 FETCH_HEAD
PASS cargo test --locked -p project range_formatting_conflicts_preserve_lsp_insert_boundaries
PASS cargo test --locked -p worktree --test integration test_global_gitignore_without_repository
PASS cargo test --locked -p language_tools tooltip_for_server_binary_handles_runtime_and_standalone_servers
PASS cargo check --locked -p gpui_linux
PASS cargo test --locked -p cli restart_cli_args_precedes_positional_paths
PASS cargo test --locked -p settings_content delay_ms_accepts_display_values
PASS cargo test --locked -p language edit_prediction_debounce_only_applies_to_manual_providers
PASS cargo check --locked -p edit_prediction -p settings_ui
PASS cargo check --locked -p zzz --features inspector
PASS cargo check --locked -p terminal -p project -p acp_thread -p search -p edit_prediction_cli -p miniprofiler_ui -p editor
PASS cargo test --locked -p terminal terminal_hyperlinks
PASS cargo test --locked -p editor test_inlay_hints_request_timeout
PASS git diff --check
FAIL cargo fmt --all --check (pre-existing formatting drift outside this batch)
NOT RUN macOS / Windows / wasm32 gpui_web runtime
NOT RUN cargo test --workspace
```

The reviewed baseline is `ab208db8d264ad08f62bda7ba0ce560c220a347f`.
Work remains on `sync/upstream-2026-08-27` and has not been merged to
`main`.

## Continuation: 2026-08-28 (fourth batch)

### Scope

- Target branch: `sync/upstream-2026-08-27` from `main` at
  `2761d2445eca441bc5948e8d35bebb01274496cf`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline:
  `ab208db8d264ad08f62bda7ba0ce560c220a347f`
- Reviewed upstream head:
  `7f2a2c3c3ee2f23f28772dee7661fb98d3910990`
- Live upstream head queried:
  `cf1900f44d30c771207e36e2c9094b6c1f659bea`
- Query time: `2026-08-28T13:26:24+02:00`

This continuation reviewed the next 20 commits after `ab208db8`. Counts:
6 A, 3 B, 11 C. The reviewed baseline is now
`7f2a2c3c3ee2f23f28772dee7661fb98d3910990`; 84 commits remain through the
queried live head.

### Decisions

| Upstream | Class | Local commit | Disposition                                                               |
| -------- | ----- | ------------ | ------------------------------------------------------------------------- |
| 875e2a1c | C     | --           | Publishing docs target deleted ZZZ extension-publishing pages.            |
| 9b5b5860 | C     | --           | `.rules` and Danger self-review automation are repository administration. |
| 075520b9 | A     | c7d094f4     | Focus Git Graph items on the search editor.                               |
| 10b2925e | A     | 85c1135b     | Document language auto-indentation rules.                                 |
| ec18126b | C     | --           | Requires the absent `mermaid_render` crate and merman dependency.         |
| 907ed09c | B     | 896b4f24     | Prevent saves and format-on-save for read-only items.                     |
| 4c763e15 | B     | 095d7216     | Preserve Git context-menu bindings during initial focus.                  |
| 53dbfe40 | C     | --           | Typed transport-error rewrite does not match ZZZ's OpenAI request APIs.   |
| 51a3ac29 | A     | 3a8241b7     | Validate extension manifest metadata before builds.                       |
| a7e23df6 | C     | --           | Attempted port failed test compilation; reverted per B-port rule.         |
| 1ea16c1a | C     | --           | Needs upstream global `GitDiffBaseSetting` toggle path absent in ZZZ.     |
| 107ee1a6 | C     | --           | Large terminal/dock restoration rewrite is unisolatable from ZZZ layout.  |
| f36aec82 | C     | --           | `ask_user` tool/default is absent from ZZZ's ACP surface.                 |
| 7316cf77 | C     | --           | Depends on deleted foreground profiler journal and bench machinery.       |
| fd82517a | A     | a5f1387c     | Add Tangled Git hosting permalinks.                                       |
| 7eec8920 | A     | 503c1b60     | Restore project LSP settings for legacy extension APIs.                   |
| d9ad6aff | A     | 201a5184     | Release X11 client state before close callbacks.                          |
| bcf033f8 | B     | a1582a51     | Clarify ZZZ Linux uninstall paths and parallel installations.             |
| 6bf539cd | C     | --           | Depends on absent blame-revision actions in ZZZ.                          |
| 7f2a2c3c | C     | --           | Requires deleted `crashes` sidecar and cross-platform quit API rewrite.   |

### Applied work

Direct A commits `075520b9`, `10b2925e`, `51a3ac29`, `fd82517a`, `7eec8920`,
and `d9ad6aff` were absorbed with `git cherry-pick -x -s` as their listed
local commits. The B ports have `Upstream`, `Retained`, and `Omitted` trailers.

- `907ed09c`: block read-only editor saves and formatting, propagate capability
  changes to tabs, and gate workspace save actions.
- `4c763e15`: retain GitPanel/ChangesList key contexts while a context menu is
  newly focused; adapt the regression test to ZZZ's four-argument API.
- `bcf033f8`: correct ZZZ uninstall examples and explain absolute paths for
  parallel installations.

### Rejected work

`875e2a1c`, `9b5b5860`, `107ee1a6`, and `7316cf77` are documentation or
repository/benchmark infrastructure that either targets deleted ZZZ paths or
cannot be isolated from rejected machinery. `ec18126b` targets the absent
`mermaid_render` crate. `f36aec82` targets an absent agent tool.

`53dbfe40` requires a typed OpenAI transport-error API that ZZZ does not have;
`1ea16c1a` requires a missing global diff-base setting; `6bf539cd` requires
missing blame-revision actions; and `7f2a2c3c` requires the deleted crash
sidecar plus a broad platform callback rewrite.

`a7e23df6` was initially attempted as a B port, but its focused test failed to
compile because ZZZ lacks the upstream `indoc` test dependency and has an
additional `ParsedMarkdown` initializer. The entire attempt was removed and
the commit was reclassified C.

### Verification

```text
PASS cargo test --locked -p workspace test_save_intents_are_noops_for_read_only_items
PASS cargo test --locked -p git_graph test_focus_handle_focuses_search_editor
PASS cargo test --locked -p git_ui test_dispatch_context_with_focus_states
PASS cargo test --locked -p extension_cli test_validate_manifest
PASS cargo test --locked -p git_hosting_providers tangled
PASS cargo check --locked -p extension_host
PASS cargo check --locked -p gpui_linux
PASS git diff --check
FAIL cargo fmt --all --check (pre-existing formatting drift outside this batch)
NOT RUN macOS / Windows / wasm32 gpui_web runtime
NOT RUN cargo test --workspace
```

The reviewed baseline is `7f2a2c3c3ee2f23f28772dee7661fb98d3910990`.
Work remains on `sync/upstream-2026-08-27` and has not been merged to
`main`.

## Fifth-batch finalization

The fifth-batch decisions and verification are recorded above. The reviewed
upstream range is `7f2a2c3c..391a66a5`, with live head
`cf1900f44d30c771207e36e2c9094b6c1f659bea`; counts are 6 A, 9 B, and 5 C.
The reviewed baseline is now
`391a66a5ad9f68e13a54f57b3b06f4605867614c`.
Work remains on `sync/upstream-2026-08-27` and has not been merged to `main`.

## Continuation: 2026-08-28 (eighth batch)

### Scope

- Target branch: `sync/upstream-2026-08-27` from `main` at
  `2761d2445eca441bc5948e8d35bebb01274496cf`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline:
  `5ed5bf95e518f1b35386a8787b512c99b90509f7`
- Reviewed upstream head:
  `01acd0ee8e906dd0ec8b526fe08da94444a5e2af`
- Live upstream head queried:
  `8ee36b682cf1971e51032cbd932dd16def575364`
- Query time: `2026-08-28T15:11:16+02:00`

This continuation reviewed the next 20 commits after `5ed5bf95`. Counts:
3 A, 5 B, 12 C. The reviewed baseline is now
`01acd0ee8e906dd0ec8b526fe08da94444a5e2af`; 6 commits remain through the
queried live head.

### Decisions

| Upstream | Class | Local commit       | Disposition                                                                           |
| -------- | ----- | ------------------ | ------------------------------------------------------------------------------------- |
| a1cc2548 | B     | 2f97a63d, 9a6bba83 | Port OpenRouter effort reasoning onto local provider APIs.                            |
| 3b90a968 | B     | 84c3db6c           | Release completed ACP PTY resources while retaining history metadata.                 |
| 8fc1a8a0 | C     | --                 | Web text-input/IME architecture conflicts with ZZZ's input path.                      |
| fc9258b4 | C     | --                 | File-permalink rewrite is not isolatable across local RPC/workspace divergences.      |
| f66ed399 | B     | 7d377066           | Clear pending key sequences when a window blurs.                                      |
| f595fd84 | C     | --                 | Upstream-only lint-creator skill documentation.                                       |
| fe5b7ced | A     | 957fbe37           | Cherry-picked loop-invariant clone cleanup; local path API preserved.                 |
| 6efd4950 | C     | --                 | Upstream-only lint-creator fixture guidance.                                          |
| c8dfe26a | A     | 81117dbe           | Cherry-picked terminal Visual Line selection support.                                 |
| d84e5d49 | C     | --                 | Depends on the rejected Web IME implementation from `8fc1a8a0`.                       |
| 12ef40c3 | C     | --                 | Broad LSP diagnostic message/protocol rewrite conflicts with local diagnostics.       |
| 5218009a | C     | --                 | Depends on the rejected Web IME implementation.                                       |
| ec312b27 | C     | --                 | Large call-hierarchy crate and deleted keymap/protocol surfaces are not localizable.  |
| 1e2e422c | C     | --                 | Broad remote extension-host/settings rewrite is not isolatable locally.               |
| 8166e3d7 | C     | --                 | Cross-panel folder-indicator/settings migration conflicts with local settings APIs.   |
| 756c2b73 | A     | 356be522           | Cherry-picked Helix mark/surround motions and selection preservation.                 |
| 797e5dc9 | C     | --                 | Bookmark-tab lifecycle rewrite requires deleted navigation and divergent editor APIs. |
| fa8d0b77 | C     | --                 | Targets the deleted crashes/telemetry crate.                                          |
| 4c6c4750 | B     | 00754a85           | Port Windows PowerShell and Git Bash discovery into `util::shell`.                    |
| 01acd0ee | B     | d565dd8a           | Port outline buffer typography and reparsed syntax highlighting; omit call hierarchy. |

### Applied work

Direct A commit `fe5b7ced` was absorbed with `git cherry-pick -x -s` as
`957fbe37`. The local path-trie hunk was adapted to ZZZ's `util::RelPath`
API. Direct A commit `c8dfe26a` was absorbed as `81117dbe`; its Visual Line
selection event uses ZZZ's `(Selection, Point)` representation. Commit
`756c2b73` was absorbed as `356be522`.

The B ports carry `Upstream`, `Retained`, and `Omitted` trailers:

- `a1cc2548`: added model capability metadata, adaptive reasoning requests,
  effort-level discovery, and a manual settings field while retaining ZZZ's
  local credentials UI and language-model trait.
- `3b90a968`: made the PTY sender releasable after ACP output capture, while
  preserving process metadata and omitting the native-agent `TerminalMode`
  rewrite.
- `f66ed399`: changed blur/focus paths to clear pending keystrokes and defer
  observer notification, updating all affected ZZZ callers.
- `4c6c4750`: moved the PowerShell/Git Bash discovery improvements into the
  existing `util::shell` module and guarded restart when PowerShell is absent.
- `01acd0ee`: shared buffer text styling, added syntax-run resolution, and
  reparsed document-symbol labels when source-buffer highlights are missing.

### Rejected work

- `8fc1a8a0`, `d84e5d49`, and `5218009a` introduce a Web IME mirror/lifecycle
  that conflicts with ZZZ's existing HTML input path.
- `fc9258b4` requires a broad permalink/RPC/workspace rewrite across divergent
  local files; `12ef40c3` similarly replaces the diagnostic message model and
  wire protocol.
- `f595fd84` and `6efd4950` only update an upstream skill absent from ZZZ.
- `ec312b27` adds a large call-hierarchy crate plus deleted keymap and protocol
  surfaces; `1e2e422c` is a broad remote extension-host rewrite.
- `8166e3d7` and `797e5dc9` require cross-panel settings/editor lifecycle
  rewrites that cannot be isolated on current ZZZ APIs.
- `fa8d0b77` targets ZZZ's deleted crash-reporting/telemetry crate.

### Verification

```text
PASS git merge-base --is-ancestor 5ed5bf95 FETCH_HEAD
PASS cargo check --locked -p open_router -p language_models
PASS cargo check --locked -p terminal -p acp_thread -p project
PASS cargo check --locked -p gpui -p editor -p workspace -p project_panel -p settings_ui -p sidebar -p vim
PASS cargo check --locked -p util -p gpui_windows
PASS cargo check --locked -p ui -p language -p syntax_theme -p outline -p editor
PASS git diff --check
FAIL cargo fmt --all --check (pre-existing formatting drift outside this batch)
NOT RUN macOS / Windows / wasm32 gpui_web runtime
NOT RUN cargo test --workspace
```

The reviewed baseline is `01acd0ee8e906dd0ec8b526fe08da94444a5e2af`.
Work remains on `sync/upstream-2026-08-27` and has not been merged to
`main`.

## Continuation: 2026-08-28 (sixth batch)

### Scope

- Target branch: `sync/upstream-2026-08-27` from `main` at
  `2761d2445eca441bc5948e8d35bebb01274496cf`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline:
  `391a66a5ad9f68e13a54f57b3b06f4605867614c`
- Reviewed upstream head:
  `5dd0666dfb627bb7c04b210b730005628f5a229a`
- Live upstream head queried:
  `351899e53107db60ba27dce7b94fa55d47b769c4`
- Query time: `2026-08-28T14:29:33+02:00`

This continuation reviewed the next 20 commits after `391a66a5`. Counts:
5 A, 8 B, 7 C. The reviewed baseline is now
`5dd0666dfb627bb7c04b210b730005628f5a229a`; 45 commits remain through the
queried live head.

### Decisions

| Upstream | Class | Local commit      | Disposition                                                                 |
| -------- | ----- | ----------------- | --------------------------------------------------------------------------- |
| 5631830c | C     | --                | Depends on the unabsorbed Bedrock Mantle model surface.                     |
| 4ca90055 | C     | --                | Broad tree-sitter/Wasmtime and lockfile/toolchain update.                   |
| e9735934 | B     | 16b379a3          | Port preview typography onto the existing Markdown renderer.                |
| 41d72f6c | B     | 8dbddbc6          | Make stacksafe opt-in while retaining ZZZ desktop protection.               |
| 91ebd505 | B     | cd2645ca          | Serialize the latest selected ACP agent task.                               |
| 693140ab | B     | 6b352045          | Accept nullable OpenAI prompt-cache usage fields.                           |
| 96382d8e | B     | d72af3b5, 261dc08 | Add ACP agent retry controls.                                               |
| 38c5dd7c | A     | b5099ab8          | Default missing Mistral stream `object` values.                             |
| b20c4694 | A     | 91612188          | Account for removed lines when diffing overwritten files.                   |
| 0969b0db | B     | 9ae568ad          | Ignore uninstalled custom agents during restoration.                        |
| 224373ab | C     | --                | Web IME/input rewrite conflicts with ZZZ's web window path.                 |
| f42c6e87 | B     | aa309ef8          | Use unstable sorting on existing deduplication paths.                       |
| 3c2848e9 | C     | --                | Requires the absent provider error-category/native-agent API.               |
| 1ff7cb66 | B     | c267bd7b          | Reuse measured glyph images in local cosmic-text paths.                     |
| 55007f51 | C     | --                | Adds telemetry reporting and profiler surfaces rejected by ZZZ.             |
| e0f913b0 | C     | --                | Targets upstream `tabular_data_preview`, replaced locally by `csv_preview`. |
| d4cd310e | C     | --                | Modifies the deleted upstream action-profiler module.                       |
| 7040aa56 | A     | b2c46bf7          | Clear Windows COLR emoji render targets before compositing.                 |
| 081a45f6 | A     | 46266953          | Add Grok 4.5 and 4.6 manual-provider models.                                |
| 5dd0666d | A     | ce84da0d          | Terminate X11 `WM_CLASS` with a second NUL byte.                            |

### Applied work

Direct A commits `38c5dd7c`, `b20c4694`, `7040aa56`, `081a45f6`, and
`5dd0666d` were absorbed with `git cherry-pick -x -s`. The B ports have
`Upstream`, `Retained`, and `Omitted` trailers in their local commits.

- `e9735934`: applied preview body sizing, line-height, spacing, table padding,
  muted quotes, and stronger heading styles; retained the local renderer.
- `41d72f6c`: made `stacksafe` optional, gated recursive wrappers, and enabled
  the feature for ZZZ desktop/test builds while preserving local feature names.
- `91ebd505`: retained the latest selected-agent persistence task to prevent
  out-of-order global KVP writes.
- `693140ab`: added nullable prompt-cache usage details and cache-aware token
  accounting to the existing OpenAI chat stream mapper.
- `96382d8e`: added Retry/Reload controls through ZZZ's connection store and
  reset path, split into two local commits for panel and conversation changes.
- `0969b0db`: validated custom ACP registrations before restoring selections and
  used a safe fallback for source-panel initialization.
- `f42c6e87`: adapted the deleted `bracket_ranges.rs` hunk to ZZZ's merged
  `language/src/buffer.rs` path and changed the other five sort sites.
- `1ff7cb66`: cached non-empty glyph images between bounds measurement and
  rasterization in the local cosmic-text implementation.

### Rejected work

- `5631830c` cannot be isolated because ZZZ does not contain the upstream
  Bedrock Mantle model family introduced by its parent history.
- `4ca90055` is a broad dependency, lockfile, tree-sitter, and Wasmtime update
  with no independent ZZZ behavior.
- `224373ab` introduces a new IME mirror and web lifecycle architecture that
  conflicts with ZZZ's existing HTML input path.
- `3c2848e9` needs provider billing/content-policy categories and native-agent
  error handling absent from ZZZ.
- `55007f51` reports frame timing data through telemetry, outside the
  no-telemetry boundary, and also targets profiler fields absent locally.
- `e0f913b0` targets the replaced `tabular_data_preview` crate.
- `d4cd310e` modifies `crates/gpui/src/profiler/actions.rs`, deleted in ZZZ.

### Verification

```text
PASS git merge-base --is-ancestor 391a66a5 FETCH_HEAD
PASS cargo check --locked -p zzz
PASS cargo check --locked -p agent_ui
PASS cargo check --locked -p markdown
PASS cargo check --locked -p open_ai
PASS cargo check --locked -p gpui_wgpu -p gpui_windows -p x_ai -p mistral
PASS cargo test --locked -p open_ai stream_usage_with_null_prompt_cache_tokens_is_not_an_error
PASS cargo test --locked -p action_log test_overwriting_file_counts_removed_lines
PASS git diff --check
FAIL cargo fmt --all --check (pre-existing formatting drift outside this batch)
NOT RUN macOS / Windows / wasm32 gpui_web runtime
NOT RUN cargo test --workspace
```

The reviewed baseline is `5dd0666dfb627bb7c04b210b730005628f5a229a`.
Work remains on `sync/upstream-2026-08-27` and has not been merged to `main`.

## Continuation: 2026-08-28 (seventh batch)

### Scope

- Target branch: `sync/upstream-2026-08-27` from `main` at
  `2761d2445eca441bc5948e8d35bebb01274496cf`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline:
  `5dd0666dfb627bb7c04b210b730005628f5a229a`
- Reviewed upstream head:
  `5ed5bf95e518f1b35386a8787b512c99b90509f7`
- Live upstream head queried:
  `8ee36b682cf1971e51032cbd932dd16def575364`
- Query time: `2026-08-28T14:46:58+02:00`

This continuation reviewed the next 20 commits after `5dd0666d`. Counts:
5 A, 0 B, 15 C. The reviewed baseline is now
`5ed5bf95e518f1b35386a8787b512c99b90509f7`; 25 commits remain through the
queried live head.

### Decisions

| Upstream | Class | Local commit | Disposition                                                               |
| -------- | ----- | ------------ | ------------------------------------------------------------------------- |
| 74490dac | A     | 5e77869b     | Cherry-picked with `-x -s`.                                               |
| 4278ff36 | A     | f625b502     | Cherry-picked with `-x -s`.                                               |
| 1d7e5f1d | A     | a7bef4c3     | Cherry-picked with `-x -s`.                                               |
| 76478024 | C     | --           | Needs the absent local provider error-category API.                       |
| b415b5d4 | C     | --           | GitHub maintainer-edit workflow automation only.                          |
| 5f1f7e63 | C     | --           | Web IME architecture conflicts with ZZZ's input path.                     |
| 1f7eae7f | C     | --           | Benchmark/test-support and upstream skill scaffolding only.               |
| 45cd77bb | A     | dd8fa365     | Cherry-picked with `-x -s`.                                               |
| fecc3273 | C     | --           | Depends on benchmark headless-renderer infrastructure.                    |
| 4ccbcabf | A     | 5dc66f10     | Cherry-picked with `-x -s`.                                               |
| 1475887f | C     | --           | Broad project-search-on-type rewrite is not isolatable locally.           |
| 839ab7a9 | C     | --           | Shallow-diff UI/protocol rewrite needs absent git APIs and askpass UI.    |
| 279da638 | C     | --           | Broad persistence shutdown rewrite conflicts with local lifecycle APIs.   |
| c595091b | C     | --           | Dependency, lockfile, and crate-graph churn without independent behavior. |
| 34241d5f | C     | --           | LLVM/settings deserializer rewrite failed on removed ZZZ fields.          |
| ea701d40 | C     | --           | Agent persistence worker targets deleted/diverged native-agent paths.     |
| 4a9a863c | C     | --           | Auto-indentation patch requires deleted `editor/src/input.rs`.            |
| ac099b4a | C     | --           | Release-version and lockfile metadata only.                               |
| 7ecab315 | C     | --           | Depends on the rejected project-search rewrite from `1475887f`.           |
| 5ed5bf95 | C     | --           | Stream-mapping rewrite spans deleted and conflicting provider APIs.       |

### Applied work

Direct A commits `74490dac`, `4278ff36`, `1d7e5f1d`, `45cd77bb`, and
`4ccbcabf` were absorbed with `git cherry-pick -x -s`. No B port was retained
in this batch.

- `74490dac`: implemented Windows DirectX headless `render_to_image` under
  the existing test-support surface.
- `4278ff36`: prevented wrapped lines from starting with closing punctuation.
- `1d7e5f1d`: corrected secondary-monitor placement with differing DPI.
- `45cd77bb`: refreshed the debugger panel when breakpoint state changes.
- `4ccbcabf`: removed the unused `line_beginning` soft-boundary parameter and
  updated Vim callers.

### Rejected work

- `76478024` cannot be isolated because ZZZ's OpenAI completion mapper no
  longer exposes the upstream `ProviderErrorCategory` path.
- `b415b5d4` is GitHub maintainer-edit automation, outside product scope.
- `5f1f7e63` introduces an IME-mirror lifecycle absent from ZZZ's web input
  implementation.
- `1f7eae7f` and `fecc3273` are benchmark/test-support infrastructure and
  upstream skill plumbing without an independent editor behavior change.
- `1475887f` is a large project-search architecture and settings rewrite;
  `839ab7a9` additionally requires missing shallow-history and askpass APIs.
- `279da638` is a broad workspace shutdown/persistence lifecycle rewrite,
  while `c595091b` is dependency-only churn.
- `34241d5f` was attempted as a local optimization port but `cargo check`
  failed because ZZZ removed `reduce_motion` and `telemetry` fields; the
  attempt was fully reverted and reclassified C.
- `ea701d40` targets native-agent session structures deleted or substantially
  diverged in ZZZ. `4a9a863c` targets the deleted upstream `editor/src/input.rs`.
- `ac099b4a` only bumps release metadata. `7ecab315` depends on the rejected
  project-search rewrite. `5ed5bf95` cannot be isolated across the deleted
  Copilot/subscribed providers and conflicting local stream APIs.

### Verification

```text
PASS git merge-base --is-ancestor 5dd0666d FETCH_HEAD
PASS cargo check --locked -p settings_content
PASS cargo check --locked -p gpui -p gpui_windows -p debugger_ui -p editor -p vim
PASS cargo check --locked -p zzz
PASS cargo test --locked -p gpui test_is_word_char
PASS git diff --check
FAIL cargo fmt --all --check (pre-existing formatting drift outside this batch)
NOT RUN macOS / Windows / wasm32 gpui_web runtime
NOT RUN cargo test --workspace
```

The reviewed baseline is `5ed5bf95e518f1b35386a8787b512c99b90509f7`.
Work remains on `sync/upstream-2026-08-27` and has not been merged to `main`.

## Eighth-batch finalization

The eighth-batch decisions and verification are recorded above. The reviewed
upstream range is `5ed5bf95..01acd0ee`, with live head
`8ee36b682cf1971e51032cbd932dd16def575364`; counts are 3 A, 5 B, and 12 C.
The reviewed baseline is now
`01acd0ee8e906dd0ec8b526fe08da94444a5e2af`; 6 commits remain through the
queried live head. Work remains on `sync/upstream-2026-08-27` and has not been
merged to `main`.
