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
- Reviewed upstream head: `b961b4950febbc050081554bafe976b5d1b93f39`
- Live upstream head queried: `b961b4950febbc050081554bafe976b5d1b93f39`
- Query time: `2026-09-19T10:16:54+02:00`
- Reviewed range: `490aad88..b961b495`

The first batch reviewed `490aad88..d7f28899` (5 A, 8 B, 7 C). The second
covered `d7f28899..25185402` (3 A, 5 B, 12 C). The third covered
`25185402..87a1ea30` (5 A, 5 B, 10 C). The fourth covered
`87a1ea30..45ff3371` (3 A, 3 B, 14 C). The fifth covered
`45ff3371..739fdbef` (3 A, 8 B, 9 C). The sixth covered
`739fdbef..06e889c4` (4 A, 8 B, 8 C). The seventh covered `06e889c4..9d956a09`
(1 A, 4 B, 15 C). The eighth covered `9d956a09..0d08af1e` (7 A, 4 B, 9 C). The
ninth batch covered the remaining 11 commits through the live head (4 A, 1 B,
6 C). Totals: 35 A, 46 B, and 90 C across 171 commits. The reviewed baseline
is now `b961b4950febbc050081554bafe976b5d1b93f39`; 0 commits remain through
the queried live head.

The first batch's decision rows and narrative were lost when a later batch
overwrote this report; they were restored on 2026-09-22 from commit
`35b4e80a9d`. The `71b60bba` version follow was reclassified `C` to `B` in the
same pass.

## Decisions

| Upstream | Class | Local commit       | Disposition                                                                                       |
| -------- | ----- | ------------------ | ------------------------------------------------------------------------------------------------- |
| 3384317a | C     | --                 | `util/debug-embed` / `fs_embed` are absent; enabling the feature would not compile.               |
| 6f72bdb7 | B     | 2ae663c6           | ACP thread copies plain text by default; keep i18n labels.                                        |
| e2534d23 | A     | 6059ecd9           | Cherry-picked with `-x -s`.                                                                       |
| 20d3cd1d | B     | c533f5c7, f15db659 | Join-line prefix fix; drop rust block-comment assertions that need comment overrides.             |
| ff6a6abb | A     | 8fe587d9           | Cherry-picked with `-x -s`.                                                                       |
| 10676bad | C     | --                 | Collab peer search sharing of `private_files`.                                                    |
| f29c8eaf | A     | 6abea349           | Cherry-picked with `-x -s`.                                                                       |
| 4612aa2f | C     | --                 | Collab selection broadcast skip needs `Project::is_shared` and deleted `editor/src/input.rs`.     |
| 72b02bf1 | A     | b1e2ffc9           | Cherry-picked with `-x -s`.                                                                       |
| f83313d0 | A     | e7f57596           | Cherry-picked with `-x -s`.                                                                       |
| 58962741 | B     | 675a0796           | Owning GPUI asset cache; omit wasm lock/channel Cargo.toml splits.                                |
| 71b60bba | B     | --                 | Version follow to v1.21.0; superseded by the v1.22.0 follow `57b5799a` (no separate commit).      |
| fceace0b | B     | 60f16bbe           | Emmet language-based suggestion on existing notifications; omit zed.dev URL and lockfile tests.   |
| 0690433b | B     | 61330ebc           | LSP executeCommand and showDocument; omit collab, telemetry proto IDs, and `lsp_locations`.       |
| 002161d5 | B     | b0806d4a           | Inlay hint commands on `element.rs`; omit deleted `element/mouse.rs`.                             |
| 33c6212b | C     | --                 | macOS Space restore rewrites persistence (`set_session_id`) and MultiWorkspace restore.           |
| 45077524 | C     | --                 | Idle-sleep API spans every GPUI backend, native-agent settings UI, and deleted `livekit_client`.  |
| 907b55f7 | B     | cc07c65f           | macOS `register_url_scheme` on objc2; enable `NSWorkspace` and `block2` features.                 |
| 318c664e | C     | --                 | Markdown parse-time highlight cache rewrites already-diverged `markdown.rs` (10 conflicts).       |
| d7f28899 | C     | --                 | Buffer highlight cache depends on rejected `318c664e` `ResolvedHighlights`.                       |
| 77226930 | C     | --                 | Trial copy for the $5 GPT Luna offer; onboarding, plan chips, and hosted-docs billing.            |
| 284c7240 | C     | --                 | Document-highlight dynamic registration needs upstream `dynamic_registration.rs`.                 |
| e9d2934e | A     | 24ae4df8           | Cherry-picked with `-x -s`.                                                                       |
| a3e93fff | B     | eabe3cdc           | wasi-sdk 34 with VERSION invalidation; omit bail-to-ensure log churn.                             |
| 52b2927a | A     | 1db3e67a           | Cherry-picked with `-x -s`.                                                                       |
| f9a1fc89 | C     | --                 | New `watch::snapshot` public API with no current ZZZ caller.                                      |
| 26b6a267 | C     | --                 | Upstream GitHub Actions / xtask Miri pin.                                                         |
| d3865b09 | B     | 8ea5c20a           | macOS path prompts on objc2; `MainThreadMarker::new()` instead of a platform tuple field.         |
| 290cbcb9 | B     | 02a1fd0e           | Remote `path_to_buffer_id` update on file move; omit conflicting telemetry test.                  |
| 6ad3c7f2 | C     | --                 | gpui_web keyboard-focus rewrite; `ime_mirror.rs` and related GPUI APIs are absent.                |
| 9e636045 | B     | add60ecd           | Inline assistant uses `default_model()` fallback.                                                 |
| 86b2cf96 | C     | --                 | Screen-capture `get_sources` needs new `objc2-screen-capture-kit` and `MacPlatform` marker field. |
| 595d6286 | B     | 552f00b3           | `comment_empty_lines` on `editor.rs`; omit deleted `input.rs` and vscode.json keymaps.            |
| 7e0b34ba | C     | --                 | Anthropic `ProviderErrorCategory::PaymentRequired` is absent.                                     |
| 72c53bf0 | C     | --                 | `start_external_drag` GPUI API is absent.                                                         |
| 3405c42f | C     | --                 | PET lockfile-only fork pin; local microsoft rev already diverged.                                 |
| 5a773a40 | C     | --                 | gpui_web IME autoscroll; `ime_mirror.rs` is absent.                                               |
| a57ba9b1 | A     | 7620cf28           | Cherry-picked with `-x -s`.                                                                       |
| 1a84d5d9 | C     | --                 | Folder-drag highlight fix needs absent external-drag APIs and new `on_file_drop_exit`.            |
| 25185402 | C     | --                 | Native Agent Panel terminal-thread renaming.                                                      |
| dfec59fb | C     | --                 | Filesystem debug window needs upstream `OsWatcher`; ZZZ still uses `GlobalWatcher`.               |
| d9e1c024 | C     | --                 | Long-press tooltips need absent `LongPressEvent` / `GestureTuning` GPUI APIs.                     |
| 6fff327a | C     | --                 | rustc 1.98.1 bump; prior 1.97 bump was C, plus collab Docker and flake lock.                      |
| 1c3d902f | B     | 40a7f609           | Continue diagnostics past paths with no worktree; adapt tests to local `Diagnostic.message`.      |
| 3cef3168 | C     | --                 | ACP 2.1 / futures 0.3.34; ZZZ is still on agent-client-protocol 0.12.0.                           |
| f0261834 | B     | ef800257           | `editor.code_lens.foreground` on `Option<String>` theme colors.                                   |
| 7ff8e1c2 | A     | --                 | Already equivalent: local ashpd is 0.13.10, newer than the 0.13.5 mailto fix.                     |
| d12e456b | B     | 0b35237d           | Raise Unix `RLIMIT_NOFILE` at startup; omit diverged `prevent_root_execution` copy.               |
| fffb52c6 | C     | --                 | Zed Pro upgrade prompts and payment-error telemetry.                                              |
| 5b4a2153 | B     | 69ef4cd7           | `all_font_names` lists platform families only, without fallback / `.SystemUIFont`.                |
| a936ce01 | C     | --                 | Upstream `.zzz/settings.json` inherit syntax for native-agent eval fixtures.                      |
| fb38178d | B     | d8ae5c81           | Headless Metal `render_scene_to_image` runs in an autorelease pool.                               |
| d2074f4e | C     | --                 | `PowerRequest` UAF fix; ZZZ `gpui_windows` has no power-request path.                             |
| a9cdfc99 | A     | --                 | Already equivalent: migrator tests already use raw strings / `unindent`, not `indoc`.             |
| 1a89a92e | C     | --                 | Unpaced renderer sessions rewrite a much larger upstream `bench_context`.                         |
| 3db02c2c | C     | --                 | New unused `Window` visibility / system-sleep APIs intended for hang telemetry.                   |
| a7c7219d | A     | 948e47eb           | Cherry-picked with `-x -s`.                                                                       |
| 9d272b03 | C     | --                 | GitHub Actions / xtask macOS SDK printing for release bundling.                                   |
| a2651e3b | A     | 22627461           | Cherry-picked with `-x -s`.                                                                       |
| 87a1ea30 | A     | d9e431df           | Cherry-picked with `-x -s`.                                                                       |
| d27fa556 | C     | --                 | Outline-panel deleted-file tree rewrite; 13 conflicts plus local i18n.                            |
| 7960b2a7 | C     | --                 | gpui_web Canvas font fallback; `font_weight_and_style` has no ZZZ caller.                         |
| 250b6581 | A     | a6d0420e           | Cherry-picked with `-x -s`.                                                                       |
| bf921d03 | B     | 69a2938f           | Commit-editor Cut/Copy/Paste menu; omit test that needs `simulate_next_frame`.                    |
| 3e442f25 | C     | --                 | First-class ACP tool-call names need ACP 2.1; ZZZ is on 0.12.0.                                   |
| d89e9c21 | C     | --                 | Native agent elicitation tool-call IDs.                                                           |
| d1dae815 | C     | --                 | ACP compaction capability and native-agent compaction rewrite.                                    |
| 25b5569d | B     | 211a9225           | `project_panel::OpenContextMenu`; omit absent external-drag GPUI imports.                         |
| f50ebf29 | C     | --                 | Tool-name fallbacks depend on unabsorbed first-class ACP names.                                   |
| 7cda6f05 | A     | c07e4bd2           | Cherry-picked with `-x -s`.                                                                       |
| f792c2d7 | C     | --                 | New unused `ShapedLineCursor` public API.                                                         |
| 47b8ea58 | C     | --                 | Reveal-in-panel needs `Item::active_project_path` and missing DiffMultibuffer.                    |
| d62802d4 | C     | --                 | Trunk README link exists only on the unabsorbed gpui_web gallery rewrite.                         |
| 59d996d8 | C     | --                 | Per-request output-limit scaffolding; Copilot, cloud, native agent, no ZZZ setter.                |
| cbffa0f5 | C     | --                 | `snapshot_with_edits` / `EditedBufferSnapshot` are absent.                                        |
| 9e6e1416 | C     | --                 | Dynamic font APIs plus gpui_web; depends on Canvas fallback.                                      |
| b68add5b | C     | --                 | Mutex removal depends on unabsorbed dynamic font loading.                                         |
| 7f00507e | A     | 1aabd210           | Cherry-picked with `-x -s`.                                                                       |
| ba7da93e | B     | 56bd8252           | Drop stale settings keys; keep omitted/`none` provider copy instead of Zeta.                      |
| 45ff3371 | C     | --                 | Recent-commands UX follow-up; conflicts across picker, settings, and vscode import.               |
| 69af529e | B     | 085e2797           | Context menus use `web_time::Instant` for WASM.                                                   |
| 169a2b11 | B     | 1bbd9b1f           | `"..."` splices inherited `read_only_files`; VS Code maps `files.readonlyInclude`.                |
| 9862d8ea | B     | 10381bf9           | DiffStat uses version-control added/deleted colors.                                               |
| ee1c6f8c | C     | --                 | GPUI release-notes Dangerfile and draft-release-notes automation.                                 |
| 5151c795 | B     | 90e2864d           | Clarify `agent.flexible` vs `agent.default_width`; keep local i18n keys.                          |
| 07df4386 | A     | --                 | Already equivalent: local gpui has no `ztracing` dependency.                                      |
| 0be55589 | A     | f2bddd27           | Cherry-picked with `-x -s`.                                                                       |
| 93f9fce1 | C     | --                 | Require GPUI release notes in Dangerfile; follows unabsorbed ee1c6f8c.                            |
| bf9a3601 | C     | --                 | GitHub Actions / xtask Linux runner size for migration checks.                                    |
| ee7af091 | C     | --                 | New unused `count_input_tokens` API plus hosted-cloud consent.                                    |
| aef893e2 | C     | --                 | Native Agent Panel Threads Sidebar width setting.                                                 |
| a95da07d | B     | 078a930a           | Helix paste uses before/after anchors in deleted hunks.                                           |
| db7f9cee | B     | 1e275524           | macOS normal windows track mouse via cocoa `NSTrackingArea`.                                      |
| 763924c2 | B     | a0f869e5           | Project panel autoscrolls collapsed parent folders.                                               |
| 01c555b4 | A     | 4d99ac07           | Cherry-picked with `-x -s`.                                                                       |
| a0f8fa7b | B     | 26e64013           | Windows dialogs and Credential Manager I/O leave the foreground thread.                           |
| f092e5e9 | C     | --                 | Rescan diagnostics need upstream `OsWatcher`; ZZZ still uses `GlobalWatcher`.                     |
| 46ee98a8 | C     | --                 | Stop events for Zed-hosted Gemini via `language_models_cloud`.                                    |
| c2451489 | C     | --                 | xtask GPUI crate-graph walker plus lockfile churn.                                                |
| 739fdbef | C     | --                 | Copilot token-limit metadata.                                                                     |
| de2c85f2 | B     | 8394a53c           | Qualify Windows screen-capture oneshot; keep local gpui_windows imports.                          |
| 5f1a6530 | C     | --                 | SuperGrok OAuth / subscribed xAI provider.                                                        |
| 0e7972f3 | C     | --                 | GitHub Actions merge-queue dependency check.                                                      |
| 55a43c22 | B     | 667c8de1           | Document git_panel keys ZZZ ships; skip collab and folder_indicator.                              |
| 95c0d74b | A     | 747f00e9           | Cherry-picked with `-x -s`.                                                                       |
| fe1dd2d3 | A     | b89b10ef           | Cherry-picked with `-x -s`.                                                                       |
| 53fcf4be | B     | fb6aa72f           | DeepSeek Flash 4.1 plus images; omit OpenCode subscription table.                                 |
| 5c9efb75 | B     | 6997e708           | std Mutex on GPUI queues and web mailbox; omit hello_web tests.                                   |
| 3c82de74 | A     | --                 | Already equivalent: `.github/CODEOWNERS.hold` is absent.                                          |
| e4d73588 | C     | --                 | Guild `REVIEWERS.conl` removal.                                                                   |
| f6838a7c | B     | 58b2f6dd           | Normal macOS windows use `NSTrackingActiveAlways` on cocoa tracking.                              |
| 2328e18c | C     | --                 | Move livekit/lychee/renovate/workflow config; upstream infra.                                     |
| b0db8327 | B     | 57b5799a           | Follow upstream app version to 1.22.0; package name stays zzz.                                    |
| 0968dc60 | A     | 197a4346           | Cherry-picked with `-x -s`.                                                                       |
| f0fb48c7 | C     | --                 | ForegroundJournal sleep/visibility needs unabsorbed Window APIs and hang telemetry.               |
| b0f53ad2 | B     | 7535e58c           | Inspector idle-cost cut; omit bench and tests that need absent GPUI APIs.                         |
| dc339e4f | C     | --                 | LLVM IR adapter sharing; conflicts plus deleted picker window_controls.                           |
| 9930d2c2 | C     | --                 | Bundle-config / share-generics follow-up; `.cargo/bundle-config.toml` is absent.                  |
| 67ebcd95 | B     | 14dfdec7           | Infer completion insert ranges; omit diverged remote_server tests.                                |
| 06e889c4 | C     | --                 | Native-agent subagent compaction; follows unabsorbed ACP compaction.                              |
| 52e0b848 | C     | --                 | Threads Sidebar auto-open; native agent plus MultiWorkspace.                                      |
| 59adbbe8 | C     | --                 | `lsp_results_location` picker needs absent `crates/lsp_locations`.                                |
| c6124d35 | B     | 5073131b           | Expand excerpts from deleted hunks on `editor.rs`; omit bookmarks helper test.                    |
| 0dea8c63 | C     | --                 | Point-diagnostic squiggles rewrite GPUI underline APIs and `element/header.rs`.                   |
| 3d91988e | C     | --                 | New `columnar_selection.rs` grapheme engine; local column selection is inline.                    |
| 70a74b87 | C     | --                 | GitHub Actions / xtask remote-server check on each commit.                                        |
| 8eebe1ce | B     | a1538e0b           | Markdown preview tab tooltip reuses the source editor path.                                       |
| cd78c2db | C     | --                 | Flaky bracket test does not exist locally.                                                        |
| 395fbd11 | C     | --                 | Auto-update title-bar feedback; `auto_update` and `UpdateVersion` are absent.                     |
| 87f65de6 | C     | --                 | gpui_web canvas emoji fallback; follows unabsorbed Canvas fallback.                               |
| 6c9d10cb | C     | --                 | WebGL texel loading; WebGL backend is absent.                                                     |
| 7a01ac15 | A     | 856d42ab           | Cherry-picked with `-x -s`.                                                                       |
| 4b47ceb9 | C     | --                 | Screen-capture objc2 migration depends on rejected `86b2cf96`.                                    |
| 31971937 | C     | --                 | Move `corgi-patches` into tooling; local tree has no corgi patches.                               |
| 0ef92145 | B     | 1859d25a           | Owned `AtlasKey` plus shared `AtlasState`; Metal stays on `gpui_macos`.                           |
| 613a80b9 | B     | 1b5b6156           | Traffic lights keep moving during fullscreen exit on cocoa/objc2.                                 |
| aec7395e | C     | --                 | cargo-shear across 159 files including telemetry, Copilot, and `crates/path`.                     |
| 534319bd | C     | --                 | Pending-keystrokes indicator file and timeout APIs are absent.                                    |
| ecc2353d | C     | --                 | Corgi patches license softlink; follows unabsorbed `31971937`.                                    |
| 9d956a09 | C     | --                 | GitHub Actions / xtask license-check frequency.                                                   |
| 6e368ab5 | C     | --                 | Proto extension version-only bump; local is still 0.3.2.                                          |
| e8041cce | C     | --                 | GLSL extension version-only bump; local is still 0.2.3.                                           |
| b6171bcc | A     | 012d63dae1         | Cherry-picked with `-x -s`.                                                                       |
| ccadf399 | C     | --                 | GitHub Actions / xtask Miri and migration-check runner sizes.                                     |
| 68ec865b | C     | --                 | ACP elicitation keyboard nav; `elicitation.rs` is absent.                                         |
| b9419ae7 | A     | 6cf49deabe         | Cherry-picked with `-x -s`.                                                                       |
| ac6818fb | A     | 24ed0feec8         | Cherry-picked with `-x -s`.                                                                       |
| cf4deb27 | A     | b7f3c1d43e         | Cherry-picked with `-x -s`.                                                                       |
| c24e309d | B     | 7e3f660623         | Relative gpui.rc manifest path; keep local non-Windows OUT_DIR staging.                           |
| 19096d4c | A     | 50fa06fc95         | Cherry-picked with `-x -s`.                                                                       |
| 74646bf2 | A     | f57fd07277         | Cherry-picked with `-x -s`.                                                                       |
| 10fb4986 | B     | 1618d3b0e0         | GPT-6 Astra catalog plus temperature omit; no ReasoningEffort::Max.                               |
| 6872711f | C     | --                 | Native Agent Panel terminal-thread double spawn.                                                  |
| 49276a3d | C     | --                 | GitHub Actions / xtask gate tests on check_style.                                                 |
| d0ae2f05 | C     | --                 | Native-agent sidebar terminal-thread selection flicker.                                           |
| 94c997e0 | A     | 7665cf8be4         | Cherry-picked with `-x -s`.                                                                       |
| 6b95ac3d | B     | ef3cf5c0ed         | `info/exclude` uses `common_dir_abs_path`; omit remote worktree test.                             |
| 74a6468b | B     | 8daa3d9d95         | `"..."` splice for `file_scan_inclusions`; drop watcherInclude import.                            |
| 15d2fd55 | C     | --                 | Disable share-generics; `.cargo/bundle-config.toml` is absent.                                    |
| 0d08af1e | C     | --                 | Share GPUI/scheduler impls; conflicts in app.rs, view.rs, div.rs, executor.rs.                    |
| 8a3f841f | A     | 8f2361d04d         | Cherry-picked with `-x -s`.                                                                       |
| 78648aaf | C     | --                 | Collab AWS SDK rustls dependency drop.                                                            |
| 650a8d1b | A     | --                 | Already equivalent: local `write_output` builds a Processor per call.                             |
| a8535d86 | C     | --                 | Unused `LanguageModelRequest.prompt_cache_key`; native agent and Copilot.                         |
| 968be64a | B     | 9ab26bea5c         | Align `"..."` docs; ZZZ copy; exclusions default includes `.sl`/`.repo`.                          |
| 72b060af | C     | --                 | Incremental git diffs need absent `DiffMultibuffer`.                                              |
| ac729007 | C     | --                 | Wezel binary-size experiment and CI path filter.                                                  |
| 5618f439 | C     | --                 | GitHub Actions Wezel runner.                                                                      |
| c9827338 | C     | --                 | GitHub Actions Wezel Linux deps.                                                                  |
| 01448f56 | A     | a1008c6b3b         | Cherry-picked with `-x -s`. Follow-up `670fbaece7` inits i18n in tests.                           |
| b961b495 | A     | 4ec59a75f4         | Cherry-picked with `-x -s`.                                                                       |

## Batch 1 applied work

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

## Batch 1 per-commit notes

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

Bumps the Zed crate from 1.20.0 to 1.21.0. ZZZ follows the upstream app
version, so this is a version follow. It is superseded by the v1.22.0 follow
`57b5799a` and has no separate local commit.

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

## Batch 1 verification

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

## Batch 2 applied work

Direct A commits `e9d2934e`, `52b2927a`, and `a57ba9b1` were absorbed with
`git cherry-pick -x -s`.

B ports retain `Upstream`, `Retained`, and `Omitted` trailers:

- `a3e93fff`: Extension grammar compilation and `download-wasi-sdk` install
  wasi-sdk 34 and invalidate stale caches via `VERSION`.
- `d3865b09`: `MacPlatform` path open/save panels use objc2 `NSOpenPanel` /
  `NSSavePanel` and block2.
- `290cbcb9`: Remote `UpdateBufferFile` removes the previous path from
  `path_to_buffer_id`.
- `9e636045`: Inline assistant uses `LanguageModelRegistry::default_model()`.
- `595d6286`: `editor::ToggleComments` gains `comment_empty_lines` (default
  true) on the `editor.rs` path.

## Batch 2 per-commit notes

### 77226930

Updates trial onboarding, plan definitions, Zed cloud provider copy, and
hosted plans-and-pricing docs for a GPT Luna trial offer.

### 284c7240

Advertises and handles `textDocument/documentHighlight` dynamic registration.
ZZZ's `DynamicRegistrations` only tracks watched files and diagnostics. The
selector-aware text-document registration module is absent, so advertising the
capability without handling it would be incorrect.

### f9a1fc89, 26b6a267

Snapshot watch channels are a new unused API. The Miri pin is GitHub Actions
and xtask workflow infrastructure.

### 6ad3c7f2, 5a773a40

gpui_web keyboard focus and IME autoscroll both touch `ime_mirror.rs`, which
ZZZ does not have. The keyboard rewrite also spans GPUI test/window APIs.

### 86b2cf96, 72c53bf0, 1a84d5d9

Screen-capture `get_sources` adds `objc2-screen-capture-kit` and needs a
`MainThreadMarker` field on `MacPlatform`. External drag and the project-panel
highlight follow-up require `start_external_drag` / `on_file_drop_exit`.

### 7e0b34ba, 3405c42f, 25185402

Anthropic credit exhaustion maps to an absent `PaymentRequired` category.
Python PET is a lockfile-only switch to the zed-industries fork at a rev
ZZZ does not share. Terminal-thread renaming is native Agent Panel.

## Batch 2 verification

```text
PASS git merge-base --is-ancestor d7f28899 FETCH_HEAD
PASS cargo check --locked -p sqlez -p extension -p language_model -p recent_projects
PASS cargo check --locked -p editor -p project -p gpui_macos -p remote_server
PASS cargo test --locked -p extension --lib test_installed_wasi_sdk_version
PASS cargo test --locked -p editor --lib test_toggle_comment
PASS cargo test --locked -p editor --lib test_advance_downward_on_toggle_comment
PASS cargo test --locked -p project --test integration test_completion_label
PASS git diff --check
NOT RUN macOS / Windows / wasm32 runtime tests
NOT RUN cargo test --workspace
NOT RUN cargo fmt --check (known edition-2024 formatting drift on this host)
```

The reviewed baseline is `251854020a9dbea1a388bf392c9bd04fd136a557`.
Work remains on `sync/upstream-2026-09-18` and has not been merged to `main`.

## Batch 3 applied work

Direct A commits `a7c7219d`, `a2651e3b`, and `87a1ea30` were absorbed with
`git cherry-pick -x -s`. Already-equivalent A commits `7ff8e1c2` and
`a9cdfc99` made no local code change.

B ports retain `Upstream`, `Retained`, and `Omitted` trailers:

- `1c3d902f`: `merge_diagnostic_entries` continues past paths with no
  worktree instead of aborting the batch.
- `f0261834`: CodeLens text uses `editor.code_lens.foreground` and falls
  back to `text_muted`.
- `d12e456b`: `util::increase_open_file_limit` raises `RLIMIT_NOFILE`
  toward 10240 on macOS and 65536 on other Unix.
- `5b4a2153`: `TextSystem::all_font_names` returns sorted unique platform
  families, including `add_fonts`.
- `fb38178d`: headless Metal `render_scene_to_image` runs inside
  `objc::rc::autoreleasepool`.

## Batch 3 per-commit notes

### dfec59fb

Adds `OsWatcher` diagnostics recording and a 929-line debug window. ZZZ's
`fs_watcher` still uses `GlobalWatcher` with separate native/poll backends,
so the snapshot APIs do not isolate.

### d9e1c024

Long-press tooltip activation dispatches `LongPressEvent` and uses
`GestureTuning`. Those GPUI types are absent.

### 6fff327a, 3cef3168, 9d272b03

The rustc 1.98.1 bump includes collab Dockerfiles and depends on the
rejected 1.97 bump. ACP 2.1 is not portable onto local 0.12.0. macOS SDK
printing is release-workflow infrastructure.

### fffb52c6, a936ce01, 3db02c2c

Payment errors still drive Zed Pro upgrade UI and telemetry. Project
settings inherit syntax targets native-agent eval fixtures. Window
visibility / `on_system_sleep` have no product caller; the intended
consumer is hang telemetry.

### d2074f4e, 1a89a92e

Windows `PowerRequest` does not exist locally. Unpaced renderer sessions
rewrite `bench_context` far beyond the local bench helper.

## Batch 3 verification

```text
PASS git merge-base --is-ancestor 25185402 FETCH_HEAD
PASS cargo check --locked -p project -p settings_content -p theme
PASS cargo check --locked -p theme_settings -p editor -p util
PASS cargo check --locked -p gpui -p gpui_wgpu -p gpui_linux
PASS cargo test --locked -p project --test integration test_diagnostic_batches_skip_paths_without_worktrees
PASS cargo test --locked -p theme_settings --lib code_lens_foreground
PASS cargo test --locked -p gpui_wgpu --lib all_font_names_tracks_available_families
PASS cargo test --locked -p editor --lib test_gutter_button_tooltip
PASS cargo test --locked -p gpui_linux --lib drag
PASS git diff --check
NOT RUN macOS / Windows / wasm32 runtime tests
NOT RUN cargo test --workspace
NOT RUN cargo fmt --check (known edition-2024 formatting drift on this host)
NOT RUN cargo check -p gpui_macos (cfg(target_os = "macos") on this Linux host)
```

The reviewed baseline is `87a1ea30e819e193f8e3fcb517c6884f91ee7a9c`.
Work remains on `sync/upstream-2026-09-18` and has not been merged to
`main`.

## Batch 4 applied work

Direct A commits `250b6581`, `7cda6f05`, and `7f00507e` were absorbed with
`git cherry-pick -x -s`.

B ports retain `Upstream`, `Retained`, and `Omitted` trailers:

- `bf921d03`: commit-message editors get Cut/Copy/Paste context menus.
  Local follow-up `eaeeaf09` drops the regression test.
- `25b5569d`: `project_panel::OpenContextMenu` deploys the menu at the
  selected entry, with Menu / Shift-F10 bindings.
- `ba7da93e`: settings reference drops `features`,
  `edit_prediction_provider`, `agent_font_size`, and
  `projects_online_by_default`.

## Batch 4 per-commit notes

### d27fa556

Rewrites `outline_panel.rs` (+4489/-1263) to synthesize `ProjectEntryId`s
for deleted files. Cherry-pick hit 13 conflicts plus local i18n.

### 7960b2a7, 9e6e1416, b68add5b, d62802d4

Canvas fallback, dynamic font installation, and the mutex follow-up are
gpui_web plus unused public APIs. The Trunk README fix only exists on
that unabsorbed gallery rewrite.

### 3e442f25, d89e9c21, d1dae815, f50ebf29

First-class tool-call names and ACP compaction need ACP 2.1
(`tool_call.name`, `CompactionCapabilities`). Elicitation IDs are native
agent. Tool-name fallbacks depend on the names commit.

### 47b8ea58, cbffa0f5, f792c2d7, 59d996d8

Reveal-in-panel needs `Item::active_project_path` and missing
`DiffMultibuffer` / `StagedDiff`. Diff highlighting needs
`snapshot_with_edits`. `ShapedLineCursor` has no caller. Output-limit
plumbing is scaffolding for later Copilot/cloud/native-agent PRs.

### 45ff3371

Follow-up to unabsorbed recent-commands UX. Conflicts in picker,
`settings_content`, vscode import, and settings UI.

## Batch 4 verification

```text
PASS git merge-base --is-ancestor 87a1ea30 FETCH_HEAD
PASS cargo check --locked -p gpui -p project_panel
PASS cargo check --locked -p project -p agent_ui -p agent_servers -p git_ui
PASS cargo test --locked -p project_panel --lib test_context_menu
PASS cargo test --locked -p project_panel --lib test_panel_keeps_focus_highlight
PASS git diff --check
NOT RUN cargo test -p git_ui test_commit_editor_context_menu_clipboard_actions (omitted; needs simulate_next_frame)
NOT RUN macOS / Windows / wasm32 runtime tests
NOT RUN cargo test --workspace
NOT RUN cargo fmt --check (known edition-2024 formatting drift on this host)
```

The reviewed baseline is `45ff33717b6e539d0d2f2123b50c4224b84bd961`.
Work remains on `sync/upstream-2026-09-18` and has not been merged to
`main`.

## Batch 5 applied work

Direct A commit `0be55589` and `01c555b4` were absorbed with
`git cherry-pick -x -s`. Already-equivalent A commit `07df4386` made no
local code change.

B ports retain `Upstream`, `Retained`, and `Omitted` trailers:

- `69af529e`: ContextMenu uses `web_time::Instant`.
- `169a2b11`: `read_only_files` is a `SplicingVec`; `"..."` extends
  inherited globs. VS Code import uses `files.readonlyInclude`.
- `9862d8ea`: DiffStat labels use version-control colors.
- `5151c795`: Settings comments, UI copy, locales, and docs explain that
  `agent.default_width` applies only when `agent.flexible` is false.
- `a95da07d`: Helix paste tracks inserted ranges with anchors.
- `db7f9cee`: Normal macOS windows disable `acceptsMouseMovedEvents` and
  add an `ActiveInActiveApp` tracking area on cocoa `msg_send`.
- `763924c2`: Collapsing a directory selects that parent and autoscrolls.
- `a0f8fa7b`: Windows file/message dialogs run on a COM STA worker;
  Credential Manager I/O uses the background executor.

## Batch 5 per-commit notes

### ee1c6f8c, 93f9fce1, bf9a3601, c2451489

GPUI release-notes scaffolding and the follow-up that requires those notes
are Dangerfile / draft-release-notes automation. The smaller Linux runner
and the GPUI crate-graph walker are GitHub Actions / xtask infrastructure.

### ee7af091, aef893e2, 739fdbef, 46ee98a8

`count_input_tokens` is a new LanguageModel API with no current ZZZ caller;
implementations target hosted Anthropic/OpenAI and data-retention consent.
Threads Sidebar width is native Agent Panel. Copilot token-limit metadata
is Copilot. Gemini stop events are Zed-hosted cloud.

### f092e5e9

Replaces the rescan rate limiter on `OsWatcher`. Local `fs_watcher` still
uses `GlobalWatcher`, so the snapshot APIs do not isolate.

### db7f9cee, a0f8fa7b

macOS mouse tracking cherry-pick needed objc2 `NSTrackingArea` and
`WindowKind::AnchoredPopup`. The port keeps cocoa tracking areas. Windows
dialog drop omits `visibility_change`, which has no local callback field.

## Batch 5 verification

```text
PASS git merge-base --is-ancestor 45ff3371 FETCH_HEAD
PASS cargo check --locked -p ui -p settings_content -p worktree -p settings
PASS cargo check --locked -p inspector_ui -p project -p agent_ui
PASS cargo check --locked -p vim -p project_panel
PASS cargo test --locked -p settings_content --lib test_read_only_files_splice
PASS cargo test --locked -p settings_content --lib test_file_scan_exclusions_splice
PASS cargo test --locked -p settings --lib test_import_read_only_files
PASS cargo test --locked -p project --test integration test_read_only_files_splice_project_settings
PASS cargo test --locked -p vim --lib test_paste_in_expanded_deleted_hunk
PASS cargo test --locked -p vim --lib test_system_clipboard_crlf_paste_at_end_of_buffer
PASS cargo test --locked -p project_panel --lib test_collapse_selected_entry
PASS git diff --check
NOT RUN macOS / Windows / wasm32 runtime tests
NOT RUN cargo test --workspace
NOT RUN cargo fmt --check (known edition-2024 formatting drift on this host)
NOT RUN cargo check -p gpui_macos (cfg(target_os = "macos") on this Linux host)
NOT RUN cargo check -p gpui_windows (cfg(target_os = "windows") on this Linux host)
```

The reviewed baseline is `739fdbef762f7e514d62e1fa650c1b7cac5bbff4`.
Work remains on `sync/upstream-2026-09-18` and has not been merged to
`main`.

## Batch 6 applied work

Direct A commits `95c0d74b`, `fe1dd2d3`, and `0968dc60` were absorbed
with `git cherry-pick -x -s`. Already-equivalent A commit `3c82de74`
made no local code change.

B ports retain `Upstream`, `Retained`, and `Omitted` trailers:

- `de2c85f2`: `WindowsPlatform::screen_capture_sources` names the
  oneshot receiver with a fully qualified path.
- `55a43c22`: all-settings documents git_panel keys that ZZZ ships.
  Dock defaults for git and outline panels match `default.json`.
- `53fcf4be`: DeepSeek lists `deepseek-flash` as V4.1 Flash and
  serializes images. Docs live in `llm-providers.md`.
- `5c9efb75`: GPUI priority queues and the gpui_web mailbox use std
  Mutex/Condvar with poison recovery.
- `f6838a7c`: Normal macOS windows track mouse with
  `NSTrackingActiveAlways`.
- `b0f53ad2`: Inspector IDs and state lookups run only while a window
  inspector is open; LSP shutdown drain uses the shutdown timeout.
  Follow-ups `7fe76ef9` and `880eb5ac` drop tests that need absent
  GPUI APIs.
- `67ebcd95`: Completions without LSP edit ranges infer an insert
  range ending at the cursor.
- `b0db8327`: follow upstream app version to 1.22.0. Package name stays
  `zzz`.

## Batch 6 per-commit notes

### 5f1a6530, 0e7972f3, e4d73588, 2328e18c

SuperGrok is an OAuth subscribed xAI provider. Merge-queue dependency
checks, `REVIEWERS.conl`, and livekit/lychee/renovate moves are GitHub
Actions, guild, or release metadata.

### b0db8327

Follow upstream app version to 1.22.0 in `crates/zzz` and `Cargo.lock`.
Package name stays `zzz`. No other release-channel metadata was
imported.

### f0fb48c7, 06e889c4

ForegroundJournal sleep/visibility depends on the rejected Window
visibility APIs (`3db02c2c`) and hang telemetry. Subagent compaction
is native agent and follows unabsorbed ACP compaction.

### dc339e4f, 9930d2c2

LLVM IR adapter sharing conflicted across ACP, deleted picker
`window_controls.rs`, settings_ui, and context_menu. The rodio /
share-generics follow-up needs absent `.cargo/bundle-config.toml`.

### b0f53ad2

Product inspector and LSP shutdown changes compiled. Upstream tests
call `Entity::cached` and `Frame::clear(&mut App)`, and
`LanguageRegistry::register_language` has a different local
signature. The gpui_platform `inspector_render` bench needs absent
`bench-support`.

## Batch 6 verification

```text
PASS git merge-base --is-ancestor 739fdbef FETCH_HEAD
PASS cargo check --locked -p gpui -p inspector_ui -p lsp
PASS cargo check --locked -p deepseek -p language_models
PASS cargo check --locked -p ollama -p editor -p project
PASS cargo test --locked -p snippet_provider --lib test_register_snippets
PASS cargo test --locked -p snippet_provider --lib test_register_global_snippets
PASS cargo test --locked -p snippet_provider --lib test_get_snippets_unknown_language
PASS cargo test --locked -p deepseek --lib model_helpers_cover_built_in_and_custom_variants
PASS cargo test --locked -p language_models --lib serializes_deepseek_image_parts
PASS cargo test --locked -p gpui --lib queue::
PASS cargo test --locked -p editor --lib test_completion_without_text_edit
PASS cargo test --locked -p editor --lib test_completion_with_explicit_replace_range
PASS git diff --check
NOT RUN macOS / Windows / wasm32 runtime tests
NOT RUN cargo test --workspace
NOT RUN cargo fmt --check (known edition-2024 formatting drift on this host)
NOT RUN cargo check -p gpui_macos (cfg(target_os = "macos") on this Linux host)
NOT RUN cargo check -p gpui_windows (cfg(target_os = "windows") on this Linux host)
NOT RUN cargo test -p inspector_ui (omitted; needs absent GPUI test APIs)
```

The reviewed baseline is `06e889c439f420714543e37806402666615ed1ca`.
Work remains on `sync/upstream-2026-09-18` and has not been merged to
`main`.

## Batch 7 applied work

Direct A commit `7a01ac15` was absorbed with `git cherry-pick -x -s`.

B ports retain `Upstream`, `Retained`, and `Omitted` trailers:

- `c6124d35`: `expand_excerpts_for_direction` resolves deleted hunks;
  `range_to_buffer_ranges_with_deleted_hunks` keeps half-open boundaries
  and trailing empty excerpts; expansion skips removed paths.
- `8eebe1ce`: Markdown preview tabs reuse the source editor
  `tab_tooltip_text`.
- `0ef92145`: `PlatformAtlas::get_or_insert_with` takes an owned
  `AtlasKey`; `AtlasState` caches only successful inserts; tests and
  Linux headless use `HeadlessAtlas`.
- `613a80b9`: Normal macOS windows keep moving traffic lights while
  exiting fullscreen and restore pre-fullscreen frames.

## Batch 7 per-commit notes

### 52e0b848, 59adbbe8, 0dea8c63, 3d91988e

Threads Sidebar auto-open is native agent plus MultiWorkspace.
Honoring `lsp_results_location` on cmd-click needs
`crates/lsp_locations`. Point-diagnostic squiggles rewrite GPUI
underline exclusion APIs and `element/header.rs`. Columnar selection
extracts a grapheme engine ZZZ does not have.

### 70a74b87, 395fbd11, 87f65de6, 6c9d10cb

Remote-server CI is GitHub Actions / xtask. Title-bar "Up to Date"
needs the absent `auto_update` crate. Canvas emoji fallback and WebGL
texel loading follow rejected gpui_web / WebGL work.

### 4b47ceb9, 31971937, aec7395e, 534319bd, ecc2353d, 9d956a09

Screen-capture objc2 depends on rejected `86b2cf96`. Corgi patches and
the license softlink are absent. cargo-shear is lockfile and unused-dep
churn across rejected crates. The pending-keystrokes indicator and CI
license-check frequency have no local surface.

### cd78c2db

The flaky bracket test
`test_bracket_ranges_keep_pairs_straddling_a_chunk_boundary_amid_errors`
does not exist locally.

## Batch 7 verification

```text
PASS git merge-base --is-ancestor 06e889c4 FETCH_HEAD
PASS cargo check --locked -p gpui -p gpui_linux -p gpui_wgpu
PASS cargo check --locked -p editor -p multi_buffer -p markdown_preview
PASS cargo test --locked -p gpui --lib atlas_tests
PASS cargo test --locked -p gpui_wgpu --lib wgpu_atlas::
PASS cargo test --locked -p editor --lib cursor_animation::
PASS cargo test --locked -p editor --lib test_cursor_animation_remains_active_during_keyboard_autoscroll
PASS cargo test --locked -p editor --lib test_expand_excerpts
PASS cargo test --locked -p multi_buffer --lib test_expand_excerpts
PASS cargo test --locked -p multi_buffer --lib test_range_to_buffer_ranges
PASS cargo test --locked -p markdown_preview --lib preview_tab_tooltip_matches_source_file_path
PASS git diff --check
NOT RUN macOS / Windows / wasm32 runtime tests
NOT RUN cargo test --workspace
NOT RUN cargo fmt --check (known edition-2024 formatting drift on this host)
NOT RUN cargo check -p gpui_macos (cfg(target_os = "macos") on this Linux host)
NOT RUN cargo check -p gpui_windows (cfg(target_os = "windows") on this Linux host)
```

The reviewed baseline is `9d956a090b411d93322bab04b64f17bf27245816`.
Work remains on `sync/upstream-2026-09-18` and has not been merged to
`main`.

## Batch 8 applied work

Direct A commits `b6171bcc`, `b9419ae7`, `ac6818fb`, `cf4deb27`,
`19096d4c`, `74646bf2`, and `94c997e0` were absorbed with
`git cherry-pick -x -s`.

B ports retain `Upstream`, `Retained`, and `Omitted` trailers:

- `c24e309d`: `gpui.rc` references `gpui.manifest.xml` next to itself;
  native Windows `embed-resource` adds that include directory.
- `10fb4986`: OpenAI API-key catalog includes `gpt-6-astra`; converters
  omit Astra temperature; `ServiceTier::Priority` deserializes `fast`.
- `6b95ac3d`: `add_path_to_git_info_exclude` writes
  `common_dir_abs_path/info/exclude`.
- `74a6468b`: `file_scan_inclusions` is a `SplicingVec`; VS Code
  `files.watcherInclude` is not imported.

## Batch 8 per-commit notes

### 6e368ab5, e8041cce

Zippy version-only bumps. Local proto is 0.3.2 and GLSL is 0.2.3, so the
patches that expected 0.3.3 and 0.2.4 conflicted. No extension source
changed.

### 68ec865b, 6872711f, d0ae2f05

ACP elicitation keyboard navigation lives in absent
`conversation_view/elicitation.rs`. The other two commits are native
Agent Panel / Threads Sidebar terminal threads.

### ccadf399, 49276a3d, 15d2fd55

Miri runner size, gating tests on `check_style`, and disabling share
generics are GitHub Actions / xtask or absent
`.cargo/bundle-config.toml`.

### 0d08af1e

Binary-size monomorphization rewrite of GPUI window/entity updates, view
rendering, and scheduler spawn. Cherry-pick conflicted in `app.rs`,
`view.rs`, `elements/div.rs`, and `scheduler/src/executor.rs`.

### 10fb4986

Local display names stay as model ids. `ReasoningEffort::Max` does not
exist, so Astra's selectable efforts stop at `XHigh`. Request conversion
tests use the local `into_open_ai` / `into_open_ai_response` signatures.

### 74a6468b

Same `"..."` merge as `read_only_files`. The upstream project-settings
worktree test needs `build_worktree` helpers that are not local.

## Batch 8 verification

```text
PASS git merge-base --is-ancestor 9d956a09 FETCH_HEAD
PASS cargo check --locked -p gpui -p open_ai -p language_models
PASS cargo check --locked -p project -p settings_content -p settings -p worktree
PASS cargo test --locked -p open_ai --lib request_conversion_omits_unsupported_temperature
PASS cargo test --locked -p open_ai --lib completion_event_decodes_priority_and_fast_service_tiers
PASS cargo test --locked -p open_ai --lib default_and_known_model_helpers
PASS cargo test --locked -p settings_content --lib test_file_scan_inclusions_splice
PASS cargo test --locked -p settings_content --lib test_file_scan_inclusions_replace_and_clear
PASS cargo test --locked -p settings --lib test_import_watcher_include
PASS cargo test --locked -p worktree --test integration test_file_scan_inclusions
PASS cargo test --locked -p gpui --lib geometry::tests::test_bounds_intersects
PASS git diff --check
NOT RUN macOS / Windows / wasm32 runtime tests
NOT RUN cargo test --workspace
NOT RUN cargo fmt --check (known edition-2024 formatting drift on this host)
NOT RUN cargo check -p gpui_macos (cfg(target_os = "macos") on this Linux host)
NOT RUN cargo check -p gpui_windows (cfg(target_os = "windows") on this Linux host)
```

The reviewed baseline is `0d08af1e53378fc2aca09eed049ab77df1a8439a`.
Work remains on `sync/upstream-2026-09-18` and has not been merged to
`main`.

## Batch 9 applied work

Direct A commits `8a3f841f`, `01448f56`, and `b961b495` were absorbed with
`git cherry-pick -x -s`. Already-equivalent A commit `650a8d1b` made no
local code change. Local follow-up `670fbaece7` inits i18n in `tasks_ui`
tests so untitled editors can be added to a pane.

B ports retain `Upstream`, `Retained`, and `Omitted` trailers:

- `968be64a`: rustdoc and all-settings use matching `"..."` wording for
  `file_scan_exclusions`, `file_scan_inclusions`, and `read_only_files`.
  Exclusions defaults include `**/.sl` and `**/.repo`.

## Batch 9 per-commit notes

### 650a8d1b

Upstream keeps a 2 MiB `Processor` on `Terminal` and drops it in
`release_pty_resources`. Local `write_output` already constructs a
`Processor` per injected write and drops it at the end of the call, so
there is no retained parse-buffer field to release.

### a8535d86

Adds unused `LanguageModelRequest.prompt_cache_key`. The only product
caller is native-agent thread cache affinity; Copilot and agent_ui
literals are `None`. Without a ZZZ setter this is a new unused API.

### 72b060af

Skips the git job queue for blob reads and drives `DiffBuffer::load`
through `DiffMultibuffer` at concurrency 16. `DiffMultibuffer` is
absent; dropping the queue without that bound would fork unbounded
`cat-file` processes.

### 78648aaf, ac729007, 5618f439, c9827338

Collab AWS TLS, Wezel binary-size experiments, and the Wezel GitHub
Actions runner are collab or upstream CI infrastructure.

### 01448f56

Product change cherry-picked cleanly. The new untitled-editor regression
test needs `i18n::init` because local tab titles call `tr`.

## Batch 9 verification

```text
PASS git merge-base --is-ancestor 0d08af1e FETCH_HEAD
PASS cargo check --locked -p editor
PASS cargo check --locked -p task -p tasks_ui -p util
PASS cargo check --locked -p settings -p settings_content -p ui
PASS cargo test --locked -p editor --lib test_autoscroll
PASS cargo test --locked -p task --lib test_worktree_root_with_spaces_stays_atomic_in_args_and_cwd
PASS cargo test --locked -p util --lib windows_powershell_preserves_spaced_arg_as_single_shell_argument
PASS cargo test --locked -p util --lib windows_cmd_preserves_spaced_arg_as_single_shell_argument
PASS cargo test --locked -p tasks_ui --lib test_non_project_active_editor_uses_visible_worktree_context
PASS cargo test --locked -p tasks_ui --lib test_default_language_context
PASS cargo test --locked -p settings_content --lib test_file_scan_exclusions_splice
PASS cargo test --locked -p settings_content --lib test_file_scan_inclusions_splice
PASS cargo test --locked -p settings_content --lib test_read_only_files_splice
PASS git diff --check
NOT RUN macOS / Windows / wasm32 runtime tests
NOT RUN cargo test --workspace
NOT RUN cargo fmt --check (known edition-2024 formatting drift on this host)
NOT RUN cargo check -p gpui_macos (cfg(target_os = "macos") on this Linux host)
NOT RUN cargo check -p gpui_windows (cfg(target_os = "windows") on this Linux host)
```

The reviewed baseline is `b961b4950febbc050081554bafe976b5d1b93f39`.
Work remains on `sync/upstream-2026-09-18` and has not been merged to
`main`.

## Follow-up: v1.22.0 version bump

`b0db8327` was reclassified from C to B so the local app version tracks
upstream. Local commit `57b5799a` updates `crates/zzz` and `Cargo.lock`
from 1.20.0 to 1.22.0.

```text
PASS cargo check --locked -p zzz
PASS git diff --check
NOT RUN macOS / Windows / wasm32 runtime tests
NOT RUN cargo test --workspace
```
