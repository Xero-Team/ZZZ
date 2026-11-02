---
title: Upstream Sync 2026-09-26
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-09-26

## Scope

- Target branch: `sync/upstream-2026-09-26` from local `main` at
  `c61879d18274e4ae23d7368d690bac758287c699`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previously reviewed baseline: `397cbc84de333eeb56849dc90782d365b37e60d7`
- Reviewed upstream head: `933d8d93819c749a607e561883855a9b95c79cea`
- Live upstream head queried: `933d8d93819c749a607e561883855a9b95c79cea`
- Query time: `2026-09-26T05:50:50+02:00`
- Requested range starts after `397cbc84de333eeb56849dc90782d365b37e60d7`

Upstream `main` had only seven commits after the previous baseline, so
this run reviews all of them, from `f8f0a28508` through `933d8d9381`.
The reviewed baseline advances to
`933d8d93819c749a607e561883855a9b95c79cea`, which is also the live head
at query time.

This run also carries an explicit user request to sync the version
number, so `f57e400a17` ("Bump Zed to v1.23.0"), previously rejected in
Run 3 of the 2026-09-24 report as release metadata, is applied locally as
`c2cd52641b`. That decision table row supersedes the earlier rejection.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                       |
| ---------- | ----- | ------------ | --------------------------------------------------------------------------------------------------------------------------------- |
| f8f0a285   | B     | e47c0a3b55   | Breadcrumb highlights reuse the source buffer outside excerpts; tests adapted to local `OutlineItem`.                             |
| adc8d7918e | C     | --           | Folder context menu depends on the rejected git-panel multi-select rewrite; local `git_panel` has diverged.                       |
| 7ea077c7   | B     | e2de8fa27b   | Blocking watcher registration and case probe move to the blocking pool.                                                           |
| e52ab15eac | C     | --           | Upstream contribution guidelines and PR template; ZZZ's CONTRIBUTING and template have diverged.                                  |
| 975845875b | C     | --           | 14k-line language-model architecture rewrite; unisolatable from the provider-crate surface and every caller site it reconfigures. |
| e91b82c106 | C     | --           | Linux headless renderer depends on the upstream renderer/bind-group refactor; 11 conflict regions on a diverged backend.          |
| 933d8d9381 | C     | --           | Follow-up to 975845875b `LanguageModelClient` split; unisolatable from that architecture rewrite.                                 |
| f57e400a17 | B     | c2cd52641b   | Version sync to 1.23.0, requested by the user; supersedes the Run 3 rejection.                                                    |

Totals: three `B`, five `C`, zero clean `A`.

## Applied work

No clean `A` was available, so no direct `git cherry-pick -x -s` commit
was created. Every `B` was committed with `git commit -s` and a `sync:`
subject with `Upstream:` / `Retained:` / `Omitted:` trailers. No remote,
pull request, or named upstream remote was created.

- `e47c0a3b55` ports the breadcrumb highlight fallback. The first
  cherry-pick conflicted only in the test module import block, so the
  change was resolved onto the local APIs and committed as `B`.
- `e2de8fa27b` wraps the case-sensitivity probe and
  `register_existing_path` in `smol::unblock` inside
  `poll_path_until_created`.
- `c2cd52641b` bumps the `zzz` package and `Cargo.lock` entry to
  `1.23.0`.

## Per-commit narrative

### f8f0a285 — B, `e47c0a3b55`

`highlights_from_buffer` previously fell back to tree-sitter highlighting
limited to the buffer excerpt, which left breadcrumb text in other
excerpts unstyled. The port reuses the source buffer's highlight offsets
when the outline text equals the buffer range, returning
`combined_highlights` when the range is visible or chunked full-buffer
highlights otherwise; only then does it fall back to the existing
search-based path. The production change applied directly. The conflict
was the test import block: upstream imports `OutlineItem::selection_range`,
`SharedString` outline text, and a `SemanticTokenHighlight::precedence`
field that ZZZ does not carry. The two new regression tests were adapted
to the local `source_range_for_text`, `String`, and token shapes and both
pass. Upstream's `selection_range` field and `SharedString` text are
absent locally and were not imported.

### adc8d7918e — C

Adds a folder context menu to the git panel tree view and rewrites
`deploy_entry_context_menu`, `effective_status_entries`,
`revert_selected`, and `revert_entries` around a new `SelectionTargetKind`
enum (~1,090 changed lines in `git_panel.rs`). It is built on the
git-panel multi-select work `f8c278352f`, which this repository already
rejected as C ("Large git-panel multi-select rewrite conflicts with ZZZ's
divergent panel") and which is absent locally. `effective_status_entries`
and the `marked_directories` selection model do not exist in the local
`git_panel`, so the commit depends on an unabsorbed prerequisite and is
not isolatable. Rule 6 applies.

### 7ea077c7 — B, `e2de8fa27b`

Retries for a missing watched path called the case-sensitivity probe and
`register_existing_path` on the async task, so a slow filesystem could
block the executor for many seconds. The port closes over the path,
sender, and pending events and offloads both calls with `smol::unblock`.
Upstream gates the offload on `Fs::is_fake` and splits the watcher into
`native_watcher` / `poll_watcher`; ZZZ's `FsWatcher` has neither an `fs`
handle nor that split, so the blocking calls are always offloaded. The
existing watcher integration test still passes.

### e52ab15eac — C

Shrinks the upstream PR template, rewrites `CONTRIBUTING.md`, edits
`docs/src/SUMMARY.md`, and adds `docs/src/development.md` and
`docs/src/development/ui-checklist.md`. ZZZ maintains its own
`CONTRIBUTING.md` with the DCO/no-CLA policy and mandatory AI review, and
its own `.github` template, both of which have diverged from upstream.
This is upstream contribution-process and guild documentation rather than
shipped editor behavior, so it is rejected.

### 975845875b — C

Turns `LanguageModel` from a trait object into a plain-data struct,
removes `ConfiguredModel`, moves request serving onto
`LanguageModelProvider`, and rewrites every provider and caller (~7,250
insertions, ~6,980 deletions). It is entangled with the cloud and
subscribed providers and is not isolatable. The local provider set has no
`language_models_cloud`, `openai_subscribed`, or `x_ai_subscribed` crate,
and the commit changes the core trait that every remaining provider
implements. A dry-run port would be a rewrite, not a port. Rule 6 applies.

### e91b82c106 — C

Adds Linux WGPU headless rendering by extracting a `WgpuRendererCore`
shared by windowed and headless wrappers and rewiring platform selection
through `bench-support` / `test-support` features (~1,270 insertions,
~550 deletions). A dry-run cherry-pick conflicted in nine files with
eleven regions in `wgpu_renderer.rs`; the diff is built on the upstream
`WgpuRenderer` / separate instance-and-texture bind-group refactor and
atlas bind-group caching that ZZZ never absorbed (a sibling change,
`a434bb7e`, was rejected as C in Run 2 of the 2026-09-24 report). The
local renderer diverges substantially, so this is an unisolatable
renderer rewrite rather than a portable feature.

### 933d8d9381 — C

Splits the request methods of `LanguageModelProvider` into a new
`LanguageModelClient` supertrait. It is a mechanical follow-up that
assumes the `975845875b` plain-data model and its provider-side request
methods, which are not present locally, so it depends on an unabsorbed
predecessor.

### f57e400a17 — B, `c2cd52641b`

The user asked for the version number to be synced. Upstream bumps its
`zed` crate to `1.23.0`. ZZZ keeps its own `zzz` package, so
`crates/zzz/Cargo.toml` and the matching `Cargo.lock` entry both move
from `1.22.0` to `1.23.0`; the `crates/zed` package identity was not
imported. This row supersedes the `f57e400a17` rejection recorded in the
2026-09-24 report.

## Verification

| Check                                                                      | Result  |
| -------------------------------------------------------------------------- | ------- |
| `git diff --check`                                                         | PASS    |
| `cargo fmt --all -- --check`                                               | PASS    |
| `cargo check --locked -p editor` (breadcrumb port)                         | PASS    |
| `cargo check --locked -p fs` (watcher port)                                | PASS    |
| `cargo check --locked -p zzz` (after the version bump)                     | PASS    |
| `cargo test --locked -p editor --lib document_symbol` (12 passed)          | PASS    |
| `cargo test --locked -p fs --test integration watch` (1 passed, 1 ignored) | PASS    |
| macOS / Windows runtime checks for platform hunks                          | NOT RUN |
| `cargo test --workspace`                                                   | NOT RUN |

The first `cargo fmt --all -- --check` after the watcher port reported
only the new closure formatting, which was fixed and folded into the
corresponding commits, so the whole-tree format check now passes with no
pre-existing drift.
