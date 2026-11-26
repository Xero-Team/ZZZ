# Upstream sync 2026-11-26 (medium queue)

## Scope

|                         |                                             |
| ----------------------- | ------------------------------------------- |
| Local base branch       | `main` at `33f8b139aa`                      |
| Work branch             | `sync/upstream-medium-2026-11-26`           |
| Upstream URL            | `https://github.com/zed-industries/zed.git` |
| Upstream ref            | `refs/heads/main`                           |
| Range start             | `831229fe70bbb126d49cde0b703ae6a8b25893d8`  |
| Reviewed head           | not advanced                                |
| Live head at query time | `a0e666ae930a85ab38947e2d49222b0ea99f1c2f`  |
| Query time              | 2026-11-26                                  |

The candidate list is the ten-row "medium queue" table in
`upstream-sync-audit-2026-11-26.md`. `9dbc9e76` (migrator) is a separate
project and is not part of this batch.

## Decisions

| Upstream     | Class   | Local commit | Disposition                                                     |
| ------------ | ------- | ------------ | --------------------------------------------------------------- |
| `1416404b88` | B       | `97d7f6ba3f` | Discarding unstaged changes keeps staged content                |
| `64d14ea8`   | B       | `95b3b8e65f` | Three-stage gpg signing fallback                                |
| `4a9a863c`   | B       | `621d602afc` | Indent after a block-comment closing line from the opening line |
| `a473ea63`   | B       | `e57186f731` | Image viewer drops the previous atlas entry on reload           |
| `4bdf188c`   | B       | `442846f3be` | Stash tracked changes without untracked files                   |
| `7ecab315`   | B       | `9482d09402` | Project search drops closed untitled buffers                    |
| `66432e4ca9` | B       | `6fd11deaed` | Canonical LSP paths map into symlinked external directories     |
| `ab7d21d4`   | C       | none         | Workspace diagnostics polling rework; local tree predates it    |
| `1e2e422c`   | pending | none         | Extension LSP settings over remote, 4645 lines                  |
| `45077524`   | pending | none         | Idle-sleep suppression during a streaming turn, 1750 lines      |

The two `pending` rows are not classified: this run ended before they were ported.
They stay unreviewed and must not be treated as rejected.

## Applied work

Every applied commit is a `B` port (`git commit -s`, with `Upstream:` /
`Retained:` / `Omitted:` trailers). No `git cherry-pick -x -s` was used, because
every one of the ten needs a local-shape port.

### `1416404b88` — keep staged changes when discarding unstaged ones

`checkout_files` now takes `Option<String>`; `None` restores the worktree from the
index and leaves the index alone. `pending_op::GitStatus::RevertedUnstaged` is new.
Local `git_panel` uses a different selection model (`marked_entries: Vec<usize>`,
`Section::{Conflict, Tracked, New}`) than the upstream `HashSet<TreeKey>` and
Staged/Unstaged sections, so only the data-safety invariant was ported.

### `64d14ea8` — sign with the system pinentry before the askpass modal

The gpg wrapper is three stages: `--pinentry-mode error`, then the default
pinentry, then loopback with the askpass modal, with stdin and output buffered.

### `4a9a863c` — do not indent from a block-comment closing line

`BufferSnapshot::block_comment_closing_indent` reports the indentation a closing
delimiter logically sits at, and `logical_indent_size_for_line` uses it, so the
newline, tab, and paste paths agree. The `crates/editor/src/input.rs` half was
omitted: ZZZ deleted that module and the shared logic is in `buffer.rs`. Two stale
assertions in `test_newline_documentation_comments` were corrected, and the five
cross-language regression tests were ported.

### `a473ea63` — drop the previous atlas entry when the image reloads

`ImageView` tracks the render image it displays and releases the previous atlas
entry, and the asset-cache entry when the source changed, before swapping in a new
decode. Test-support helpers `Image::is_asset_cached`,
`ImageSource::is_asset_cached`, `Window::has_image_atlas_entry`, and
`PlatformAtlas::contains` were added. The split-view case was omitted: the local
test window does not populate the atlas tile index, so that assertion cannot
observe the shared entry.

### `4bdf188c` — stash tracked changes without untracked files

`Repository::stash_tracked` filters cached status with `FileStatus::is_created`,
not `is_untracked`, so a staged new file stays out of the pathspec; an empty
pathspec returns early because `git stash push` with no paths stashes everything.
The overflow menu offers it only when the list is grouped by status.
`git::StashStaged` and the bundled `StageSection` / `UnstageSection` feature were
omitted: upstream gates `Stash Staged` on a `Staged & Unstaged` grouping mode that
local `GitPanelGroupBy` does not have.

### `7ecab315` — stop project search holding closed untitled buffers

`ProjectSearch` subscribes to `workspace::Event::ItemRemoved` and drops the closed
buffer's excerpts. `remove_stale_buffers` filters through `is_buffer_stale`, which
also guards both the streamed-results and excerpt paths. `BufferStore::is_shared`
backs the sharing check. The `apply_reused_chunk` hunk was omitted because local
`project_search.rs` has no excerpt-reuse path, and the `shared_with_me` half of
`is_shared` because local `RemoteBufferStore` has no such list.

### `66432e4ca9` — map canonical LSP paths into symlinked external directories

`LocalSnapshot::relative_path_for_external_abs_path` maps a canonical path back
into the worktree, and `find_worktree_for_lsp_path` consults it after
`find_worktree` misses, waiting for scanning worktrees and retrying.
Upstream's `external_canonical_to_relative` `BTreeMap` was not imported: ZZZ
already records the same mapping in `symlink_paths_by_target`, which moved from
`BackgroundScannerState` onto `LocalSnapshot` so the lookup can reach it. The
`scan_symlinks: always` test was omitted because ZZZ has no `scan_symlinks`
setting.

## Rejected

### `ab7d21d4` — workspace diagnostics polling

Class C. The commit fixes the capacity-1 refresh channel in
`lsp_workspace_diagnostics_refresh`: a second concurrent waiter resolved to
`false` immediately, and request errors dropped queued refreshes. Local
`crates/project/src/lsp_store.rs` still carries the _earlier_ single-waiter design
(`let Some(mut completion_tx) = refresh_rx.recv().await` with a `take()`d
`Option<oneshot::Sender<bool>>`), not the `completion_txs: Vec<_>` plus
`queued_completion_txs` queue that this commit repairs. The defect it fixes does
not exist in ZZZ, and adopting the fix means first absorbing the queued-waiter
rework it builds on, which this batch does not contain. Isolation Test item 6:
depends on an unabsorbed earlier commit.

The `ResponseError` / `ResponseErrorCode` machinery that the commit introduces in
`crates/lsp/src/lsp.rs` was written, compiled, and tested here, then reverted
whole: without the `lsp_store` consumers it would be a new public API with no
local caller.

## Verification

| Target                                            | Check                                                   | Result     |
| ------------------------------------------------- | ------------------------------------------------------- | ---------- |
| `git`, `fs`, `project`, `git_ui`, `project_panel` | `cargo check`                                           | PASS       |
| `git`                                             | `test_checkout_files_from_index_keeps_staged_changes`   | PASS       |
| `git_ui`                                          | `test_discard`                                          | PASS       |
| `askpass`                                         | `cargo test -p askpass`                                 | PASS (4)   |
| `language`                                        | `test_autoindent`                                       | PASS (16)  |
| `editor`                                          | five new cross-language tests                           | PASS (5)   |
| `editor`                                          | `test_newline_documentation_comments`                   | PASS       |
| `gpui`, `image_viewer`                            | `cargo check`                                           | PASS       |
| `image_viewer`                                    | `cargo test -p image_viewer`                            | PASS (3)   |
| `git_ui`                                          | `cargo test -p git_ui --lib`                            | PASS (132) |
| `search`                                          | `cargo test -p search --lib`                            | PASS (65)  |
| `worktree`                                        | `test_relative_path_for_external_abs_path`              | PASS       |
| `project`                                         | `test_open_buffer_via_lsp_maps_external_symlink_target` | PASS       |
| `ab7d21d4`, `1e2e422c`                            | not run                                                 | NOT RUN    |
| `45077524` macOS and Windows backends             | not run                                                 | NOT RUN    |

## Outstanding

1. `1e2e422c` and `45077524` are unported (6395 lines together). Both are
   philosophy-safe `B` candidates: `gpui::Platform::prevent_idle_sleep` already
   exists locally as a no-op default, so `45077524` needs the four platform
   backends rather than a new API. Neither has a local commit, so neither is
   absorbed.
2. The discriminating `**/.ssh/config` fixture test for `2240542744` is not
   written; `language_for_file` itself already reads `file_system_abs_path`.
3. Three ledger items from `upstream-sync-audit-2026-11-26.md` are open:
   reclassifying `3ab0b5df18`, the `upstream-sync-audit-*.md` skip in
   `script/backfill-upstream-ledger`, and the ten stale local commit refs in
   `upstream-sync-audit-2026-09-22.md`.
4. The reviewed baseline is **not** advanced, because `1e2e422c` and `45077524`
   still have no class.
