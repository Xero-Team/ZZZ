---
title: Upstream Sync 2026-10-24
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-10-24

## Scope

- Target branch: `sync/upstream-2026-10-24`, branched from `main` at
  `3aacdc4ee7`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `3dbf6a8101b0032e5c3e9061bdc756086d971b31`
- Reviewed head: `b7d50b95e8db53607438ef6f5c29d55b7b38f852`
- Live upstream head queried: `79fe6a645f68f8c9230578f154bf99852d9f716f`
- Query time: `2026-10-24T00:30:28+02:00`
- Reviewed range: the first 20 commits after `3dbf6a8101`, oldest first
- Remaining after this batch: 31 commits through the queried live head

The previous reviewed baseline is an ancestor of `FETCH_HEAD`. The range
through the live head contains 51 commits; this batch covers the first 20
as the configured batch size.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                              |
| ---------- | ----- | ------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 9e65fc7682 | C     | --           | Rewrites `acp_thread` diff materialization around `DiffPatch` and v2 types. ZZZ carries the simplified 455-line `diff.rs` and deleted `agent/src/tools`. |
| f41ac64812 | C     | --           | Lazy thread markdown depends on the previous lazy-diff layout and rewrites GPUI list virtualization; the local list and markdown modules have diverged.  |
| d27ad54dd6 | B     | 248e6867e3   | Resolves the on-type formatting position after pending autoindent using the applied edit id. The editor cursor-pinning half is omitted.                  |
| 2afec34882 | B     | f799cfc91f   | Pane reordering only emits `AddItem` for new or replaced items. The markdown-preview auto-open hunks target a feature ZZZ removed.                       |
| cd1255fba1 | C     | --           | Automates GitHub PR cleanup. Community/CI automation is out of scope.                                                                                    |
| 59dd60d4dd | C     | --           | Refactors `ExtensionLanguageProxy` in preparation for an unabsorbed extension PR; it does not change local behavior and rewrites the registry.           |
| cb593458e1 | C     | --           | Matches document filter patterns in per-registration dynamic registrations. ZZZ has no `lsp_store/dynamic_registration.rs` or per-registration model.    |
| 63089a6ebe | C     | --           | Watch cleanup depends on `watched_abs_path` and `external_canonical_to_relative`; ZZZ uses `symlink_paths_by_target` event remapping instead.            |
| 42c4caa474 | B     | 4d829537b1   | Passes SSH usernames verbatim via `-l` and `--`. Unrelated probe-test helpers were omitted from the diverged test module.                                |
| 544780422a | A     | b742004bc7   | Clean absorption: HTTP fallback mirror for the Ubuntu toolchain PPA in `script/linux`.                                                                   |
| b488c4e814 | B     | b7b7c97e87   | Applies the markdown `min_w_0` wrapping fix; ZZZ already depends on Taffy 0.15 and uses `compute_leaf_layout`.                                           |
| 130f1e92a4 | B     | 69b7217d53   | Updates the managed Node runtime to v24.22.1. The lockfile security refresh was omitted because local versions have diverged.                            |
| 87c92e8c09 | C     | --           | Adds `.agents` to `.gitignore`. ZZZ intentionally tracks `.agents/skills`.                                                                               |
| 0a39cdb4f3 | A     | de0e9ce79a   | Clean absorption: `window_positioning` example uses display bounds on non-primary displays.                                                              |
| 0eb085876e | C     | --           | Adds recent commit messages to AI commit-message generation. ZZZ has no `generate_commit_message` or `commit_message_prompt` path.                       |
| 3389d87577 | B     | 00ecbc2dac   | Caches direct, fallback, and missing font resolutions. The gpui_wgpu concurrency tests were omitted.                                                     |
| d86aa6e6b9 | C     | --           | Migrates macOS application and menu bindings to objc2 on a diverged `gpui_macos`; the Linux host cannot build or verify it.                              |
| faf8f0b698 | C     | --           | Logs model-fetch failures in `crates/language_models`. ZZZ does not carry that crate or its provider tree.                                               |
| 10cfb45944 | C     | --           | Migrates macOS window and appearance bindings to objc2 on the same diverged platform layer; cannot be verified on this host.                             |
| b7d50b95e8 | A     | 4a626b787b   | Clean absorption: only warn for explicitly requested fonts that fail to resolve.                                                                         |

Totals: three `A`, six `B`, eleven `C` across twenty commits.

## Applied Work

- `b742004bc7` is a direct `git cherry-pick -x -s` of `544780422a`.
- `de0e9ce79a` is a direct `git cherry-pick -x -s` of `0a39cdb4f3`.
- `4a626b787b` is a direct `git cherry-pick -x -s` of `b7d50b95e8`.
- `248e6867e3` is a `B` port of `d27ad54dd6`.
- `f799cfc91f` is a `B` port of `2afec34882`.
- `4d829537b1` is a `B` port of `42c4caa474`.
- `b7b7c97e87` is a `B` port of `b488c4e814`.
- `69b7217d53` is a `B` port of `130f1e92a4`.
- `00ecbc2dac` is a `B` port of `3389d87577`.

## Per-Commit Narrative

#### 9e65fc7682 — lazy tool-call diffs (C)

The commit moves diff buffer construction behind `Diff::resolve` and
`AcpThread::resolve_tool_call_diffs`, changing `crates/acp_thread/src/diff.rs`
from the eager `FinalizedDiff` shape. ZZZ's `diff.rs` is the older simplified
implementation without `DiffPatch`, v2 ACP types, or `edit_session.rs`, so the
patch conflicts in every touched file and cannot be isolated.

#### f41ac64812 — lazy thread markdown (C)

Adds `MarkdownOptions::defer_parsing` and rewrites the GPUI list element to
defer layout-triggered parsing. It builds on the lazy-diff work above and its
`crates/gpui/src/elements/list.rs` changes do not match ZZZ's virtualized list
implementation.

#### d27ad54dd6 — on-type formatting after autoindent (B)

`wait_for_autoindent_applied` now delivers the id of the edit that applied the
pending indentation. `LspStore::on_type_format` anchors the request position and
`on_type_format_impl` rebases it to the end of that edit before sending
`textDocument/onTypeFormatting`. This is the shared half of the fix; ZZZ
consolidated the upstream `editor/src/completions.rs` cursor pinning into
`editor.rs` without the pinning path, so that half and its editor tests were
omitted. The buffer edit-id plumbing and the anchor-based position are retained.

#### 2afec34882 — reordered tabs must not look newly added (B)

`Pane::add_item_inner` now tracks whether a move replaced a different item and
only emits `Event::AddItem` for genuinely new or replaced items. The pane tests
(`test_move_item`, `test_move_item_to_different_pane`) were retained. The
`markdown_preview` hunks extend `SuppressedAutoPreviews` for the
`open_markdown_files_in_preview` subscription, which ZZZ does not carry, so the
whole preview file and its tests were omitted.

#### 63089a6ebe — symlink watch cleanup (C)

The fix records canonical watch mappings before `read_dir` can fail, using
`watched_abs_path` and `external_canonical_to_relative`. ZZZ's scanner has a
different symlink model built on `symlink_paths_by_target` and event remapping,
and its `FakeFs` has no watch-registration accessors, so neither the behavior
nor the regression test can be isolated onto the current scanner.

#### 42c4caa474 — SSH usernames verbatim (B)

`SshConnectionOptions::ssh_destination_args` returns `-l <username> -- <host>`
instead of a percent-encoded `username@host`; `scp`/`sftp` receive `--` before
their operands. Local-only code such as `build_stdio_command` was preserved, and
the upstream probe/environment test helpers that conflict with ZZZ's trimmed ssh
test module were omitted. The destination-argument test was kept.

#### b488c4e814 — Taffy 0.15 markdown wrapping (B)

ZZZ's `gpui/Cargo.toml` already pins `taffy = "=0.15.0"` and `taffy.rs` already
calls `compute_leaf_layout`, so only the markdown change to a `min_w_0` flex
item was ported. The workspace Taffy bump and GPUI adapter changes were skipped.

#### 130f1e92a4 — managed Node security update (B)

Only `ManagedNodeRuntime::VERSION` moved from `v24.21.0` to `v24.22.1`. The
commit's wasmtime, rustls-webpki, webpki-roots, and webpki-root-certs lockfile
refresh was omitted: ZZZ pins wasmtime 49 and rustls-webpki 0.103.15, so the
lockfile portion is dependency churn rather than an isolatable behavior change.

#### 3389d87577 — font resolution cache (B)

`TextSystem` now stores `CachedFont::{Direct, Fallback, Missing}` and resolves
fonts under an upgradable read lock, so a cached lookup does not wait for a cold
platform lookup and a later font add retries previously missing families. The
`gpui_wgpu` concurrency tests assume Zed font-family names and test-only imports
that the local test module does not have, so they were reduced to the local
`font_resolution_tests` cache test.

#### 0eb085876e — recent commits in AI messages (C)

The change extends `commit_message_prompt` and `generate_commit_message` in
`git_ui`. ZZZ's panel only exposes `suggest_commit_message` and has no
prompt-building or commit-message generation path, so there is no local caller.

## Verification

| Check                                                              | Result  | Notes                                                 |
| ------------------------------------------------------------------ | ------- | ----------------------------------------------------- |
| Live `git ls-remote` and `git fetch --no-tags`                     | PASS    | `FETCH_HEAD=79fe6a645f68f8c9230578f154bf99852d9f716f` |
| Previous baseline ancestry                                         | PASS    | `3dbf6a8101` is an ancestor of `FETCH_HEAD`           |
| `cargo check --locked -p language -p project -p workspace -p gpui` | PASS    | on-type formatting and font cache                     |
| `cargo check --locked -p remote -p markdown -p node_runtime`       | PASS    | SSH, markdown wrapping, Node version                  |
| `cargo check --locked -p editor --all-targets`                     | PASS    | buffer autoindent signature change                    |
| `cargo test --locked -p editor --lib on_type_format`               | PASS    | 2 tests                                               |
| `cargo test --locked -p workspace --lib test_move_item`            | PASS    | 2 tests                                               |
| `cargo test --locked -p remote --lib transport::ssh`               | PASS    | 13 tests                                              |
| `cargo test --locked -p gpui --lib font_resolution`                | PASS    | 1 test                                                |
| `cargo fmt --check`                                                | PASS    | clean                                                 |
| `git diff --check main..HEAD`                                      | PASS    | clean                                                 |
| `./script/check-philosophy`                                        | PASS    | philosophy gate                                       |
| Locale key parity (`en.json` vs `zh-CN.json`)                      | PASS    | 3659 keys each; no new keys                           |
| macOS `gpui_macos` builds and runtime checks                       | NOT RUN | host is Linux                                         |
| Windows builds                                                     | NOT RUN | host is Linux                                         |
| `cargo test --workspace`                                           | NOT RUN | not requested                                         |
| Remaining commits through live head                                | PENDING | 31 commits; next baseline starts after `b7d50b95e8`   |

The reviewed baseline advances to
`b7d50b95e8db53607438ef6f5c29d55b7b38f852` for this batch.
