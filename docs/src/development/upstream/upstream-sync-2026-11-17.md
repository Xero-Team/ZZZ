---
title: Upstream Sync 2026-11-17
description: Selective Zed upstream sync, the first 20 of 26 commits after the 2026-11-16 batch.
---

# Upstream Sync 2026-11-17

## Scope

- Local base: `main` at `afe8038860`
- Target branch: `sync/upstream-2026-11-17`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `596dfdcf32d85d4d8d17a8887b4d0979ec7c5e14`
- Live upstream head queried: `e5be9608ddd4cb0f2f179e1c0014f0a3a0337260`
- Query time: `2026-11-17`
- Reviewed range: `596dfdcf32..d833d4689d`, the first 20 commits, oldest first
- New reviewed baseline: `d833d4689d04183d1dfd1ad862dc80f6f044a41f`
- Remaining after this batch: six commits, starting with `9925feb84c` and
  `5709f329dc`

The batch is mostly narrow performance work in `fs`, `util`, `language`,
`terminal_view`, `workspace`, and `zlog`. Three of the five rejections are
missing local surfaces rather than philosophy: the local agent crate ships no
`edit_file`/`write_file` tools, the local extension docs have no `publishing`
directory, and no local provider crate would set the model flag one commit
adds. The other two are upstream infrastructure: its `.rules`/`AGENTS.md`
instruction layout together with a dangerfile edit, and community PR board
automation.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                                                                                               |
| ---------- | ----- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 24d3772111 | A     | c7cabbeedf   | Restricts the folded-ancestor fast path in `watcher_ids_covering` to macOS. Cherry-picked intact; it repairs the guard the previously absorbed `66990`/`66973` work left on Linux.                                                                                        |
| 021c7d4c4f | A     | 083052297a   | `split_operations` consumes an iterator instead of draining a `Vec`, so splitting a buffer's operation history is linear. Applied intact.                                                                                                                                 |
| 6b72097302 | A     | fa552346bf   | `BatchedTextRun::paint` shapes from `self.text.as_str()` instead of a cloned `String`. Applied intact.                                                                                                                                                                    |
| 310bba2f2f | A     | c220fb8e5f   | `natural_sort` returns `Equal` early for equal strings. Applied intact.                                                                                                                                                                                                   |
| 125ca8672e | A     | 7fc74eafa1   | `compare_lowercase` compares ASCII by lowercased bytes, so extension sorting allocates nothing per pair. Applied intact.                                                                                                                                                  |
| 053d91cb14 | B     | 1901f3a04f   | `MessageEditor::is_empty` scans the buffer instead of copying the draft, and the thread view observer uses `Editor::is_empty`. Payload below.                                                                                                                             |
| a57e2fdc38 | C     | --           | Collapses upstream `.rules` into `AGENTS.md`, deletes `CLAUDE.md`/`GEMINI.md`, and rewrites `script/danger/dangerfile.ts`. Local `AGENTS.md` names `.rules` as the Rust/GPUI authority and `docs/.rules` as the documentation authority; the dangerfile is CI automation. |
| 6c7508a133 | B     | 491404c1ca   | Keys `WatchPaths` with `collections::FxHashMap`. Payload below.                                                                                                                                                                                                           |
| 12c1fc907f | C     | --           | Makes `always_confirm` patterns prompt for in-project `edit_file`/`write_file` edits. Local `crates/agent/src/tools/` contains only `context_server_registry.rs`, and `authorize_file_edit` does not exist, because ZZZ ships no native file-edit tools.                  |
| 430a781076 | B     | 2a8bfcfb8d   | Drops an actionless language server prompt when an identical one is visible, and moves the LSP auto-dismiss timer into the prompt. Payload below.                                                                                                                         |
| 99dd426cba | C     | --           | Adds a high-merge-rate highlight to the community PR board workflow plus `github_helpers`/`snowflake_helpers`. Upstream community and CI automation.                                                                                                                      |
| 89d3da6578 | C     | --           | Adds `LanguageModel::supports_system_prompt_updates`. Local `crates/language_model/src` holds only `api_key.rs`, `language_model.rs`, and `request.rs`, and no local provider crate would set the flag, so the field would have no setter and no caller.                  |
| d39178f6c9 | B     | 49c12b12e0   | Focuses the composer only for a manual queued-message send, so a background thread's automatic send no longer steals focus. Payload below.                                                                                                                                |
| 15ea61ea39 | B     | e1e86a482a   | `TerminalPanel` forwards its pane's `AddItem`/`RemovedItem` as workspace item events, so the tab switcher drops a closed terminal immediately. Payload below.                                                                                                             |
| f89b22bbea | A     | a2d4df842d   | Counts module filters toward `LEVEL_ENABLED_MAX_CONFIG`. Applied intact.                                                                                                                                                                                                  |
| 52b0e43064 | A     | 0a0ce7741f   | Skips the primary-selection write when every selection is empty, with a Linux-only regression test. Applied intact.                                                                                                                                                       |
| 917559da35 | A     | 968234ecd3   | Turns the thread switcher's cycling into `menu::` select actions, binds `ctrl-up`/`ctrl-down`, and adds first/last handling. Applied intact; the ported test reads local `ThreadSwitcherEntry` as a struct (one thread, no `Terminal` variant).                           |
| 7b5cc722a1 | A     | dfd53f80ec   | Already equivalent: local `:s` without `g` replaces only the first match on the line. Only the upstream vim regression test landed. Payload below.                                                                                                                        |
| d3d58c304b | C     | --           | Rewrites the MCP server extension policy in `docs/src/extensions/publishing/prerequisites.md`. Local `docs/src/extensions/` has no `publishing/` directory, and the paragraph describes the upstream extension registry.                                                  |
| d833d4689d | A     | 27f3761bdc   | Folds an ASCII path with `to_ascii_lowercase` on macOS instead of per-character NFC normalization and lowercase. Applied intact; upstream's macOS-only guard fix is the `24d3772111` commit above.                                                                        |

Totals: ten `A` (one already equivalent, one test-only), five `B`, five `C`.

## Applied Work

- `24d3772111`, `021c7d4c4f`, `6b72097302`, `310bba2f2f`, `125ca8672e`,
  `f89b22bbea`, `52b0e43064`, `917559da35`, and `d833d4689d` are direct `A`
  commits (`git cherry-pick -x -s`).
- `7b5cc722a1` is already-equivalent `A`; only its vim regression test landed.
- `053d91cb14`, `6c7508a133`, `430a781076`, `d39178f6c9`, and `15ea61ea39` are
  `B` ports, signed off for DCO.
- `917559da35` applied as a clean `A`; its commit message was rewritten and its
  ported test adapted, so it is signed off for DCO like the `B` ports.

No remote, pull request, or named upstream remote was created.

## Per-Commit Narrative

### 24d3772111 — macOS-only folded-ancestor fast path (A)

The commit flips two `cfg` attributes: the per-ancestor folded lookup becomes
`not(target_os = "macos")` and the folded-path walk becomes `target_os =
"macos"`. Local `watcher_ids_covering` still carried the older `windows`/`not
windows` split from the previously absorbed folding work, so the upstream
pre-image was intact and the cherry-pick applied after a three-way merge. This
changes nothing on Linux; the expensive per-character normalization it avoids
only exists on macOS.

### 053d91cb14 — empty-draft checks without copying the text (B)

The retained behaviour is that `MessageEditor::is_empty` walks characters from
`MultiBufferOffset(0)` until it finds a non-whitespace one, and that the thread
view's message-editor observer asks `Editor::is_empty` instead of materializing
the draft. Both `is_empty` results are unchanged: `str::trim` removes exactly
the characters `char::is_whitespace` matches, and `Editor::is_empty` reads the
buffer length.

Local adaptation: the local observer inlines the emptiness check in its `if`
instead of binding `is_empty` first, so the two hunks were ported by hand.

### 6c7508a133 — FxHashMap for watch-path registration (B)

`WatchPaths` now keys registrations with `collections::FxHashMap`, so event
dispatch hashes each `WatchKey` with the faster hasher.

Omitted: upstream's import block also pulls in `futures::Stream` and
`StreamExt`. Local `crates/fs/src/fs_watcher.rs` imports neither and uses
neither, so only `use collections::FxHashMap;` was added.

### 430a781076 — duplicate language server notifications (B)

`Workspace::show_language_server_prompt` returns early when the request has no
actions and a visible `LanguageServerPrompt` has the same `lsp_name`, `level`,
and `message`, so a repeated server message no longer stacks. The prompt now
owns its auto-dismiss timer, started in `LanguageServerPrompt::new`, instead of
the generic `show_notification` path reaching into the prompt after the fact.
`NotificationId::composite` still keys each request by its numeric id, so
action-bearing prompts stay independent.

Local adaptations: the cherry-pick conflicted on the import block, where local
keeps `svg` and no longer needs `AnyEntity`. The ported test asserts on
`Workspace::notification_ids()` because local `Workspace` exposes no
`notification_views()`; the test's own assertions are otherwise unchanged, and
`lsp` was added as a workspace test dependency for `MessageActionItem`.

### d39178f6c9 — queued-message focus (B)

Retained: `ThreadView::send_queued_message_at_index` focuses the composer only
when the send is manual (`Send Now`, or fast-tracking the next entry from an
empty composer). The automatic send from the Stopped handler no longer takes
focus away from whatever the user is typing elsewhere in the window.

Omitted: upstream's `QueuedMessageDispatchSource` enum and its separate
`message_queue` abstraction. Local keeps `local_queued_messages` and already
threads an `is_send_now` flag through every call site: the Stopped handler
passes `false`, and `send_now` plus fast-track pass `true`, which is the same
split as upstream's `Automatic`/`Manual`. The ported regression test opens a
second conversation, queues two follow-ups on the first, and asserts that
typing in the second one survives both automatic sends.

### 15ea61ea39 — terminal panel item events (B)

Retained: `TerminalPanel` re-emits its pane's `AddItem` and `RemovedItem` as
`workspace::Event::ItemAdded` and `ItemRemoved`. The workspace only emits those
for centre panes, so the tab switcher's existing workspace subscription never
saw a terminal panel item change and kept a closed terminal listed until it was
reopened.

Omitted: nothing behavioural. The cherry-pick conflicted on the test import
block, and the ported test drops the leading `bool` argument that local
`add_terminal_shell` does not take and no longer needs `WorkspaceId`.

### 7b5cc722a1 — substitute without the g flag (A, already equivalent)

Upstream fixed `one_match_per_line` in `SearchQuery::stream_matches`, which
converted match offsets that were relative to the sliced text back into buffer
positions without adding the subrange offset, so rows before the subrange were
marked as seen and several matches on one line were kept.

Local `crates/project/src/search.rs` does not do that conversion at all: the
regex branch walks line offsets and breaks out of the per-line loop after the
first match. Local vim also routes `:s` through `ReplaceCommand` and
`BufferSearchBar` rather than the project search path. Only the upstream vim
regression test was ported; it passes against local `main` with no product
change, which is the evidence that the local implementation is already correct.

### 917559da35 — thread switcher keyboard navigation (A)

`cycle_selection` and `select_last` become `menu::SelectNext` and
`menu::SelectPrevious` action handlers that also `cx.notify()`, `SelectFirst`
and `SelectLast` handlers are added, and the switcher wires all four through
`on_action`. The sidebar and the switcher's own toggle call the action handlers
now, and `ctrl-up`/`ctrl-down` are bound in all three default keymaps.

Local `ThreadSwitcherEntry` is a struct holding one thread rather than an enum
with `Thread`/`Terminal` variants, and the switcher lists threads only, but the
code hunks applied cleanly and the ported tests only touch selection order.

## Verification

| Check                                                                                  | Result  | Notes                                                                 |
| -------------------------------------------------------------------------------------- | ------- | --------------------------------------------------------------------- |
| `cargo check --locked -p workspace --tests`                                            | PASS    | Notification dedup port and its tests.                                |
| `cargo test --locked -p workspace language_server_prompt`                              | PASS    | 2 passed.                                                             |
| `cargo test --locked -p agent_ui test_auto_sending_queued_messages_preserves_focus`    | PASS    | 1 passed.                                                             |
| `cargo check --locked -p sidebar --tests`                                              | PASS    | Thread switcher action handlers and the adapted test compile.         |
| `cargo test --locked -p sidebar thread_switcher`                                       | PASS    | 2 passed.                                                             |
| `cargo test --locked -p terminal_view test_panel_terminals_emit_workspace_item_events` | PASS    | 1 passed.                                                             |
| `cargo test --locked -p vim test_replace_without_g_after_first_line`                   | PASS    | 1 passed, with no product-code change.                                |
| `cargo test --locked -p zlog`                                                          | PASS    | 17 passed.                                                            |
| `cargo test --locked -p util paths`                                                    | PASS    | 68 passed.                                                            |
| `cargo test --locked -p editor test_empty_selections_keep_primary_selection`           | PASS    | 1 passed.                                                             |
| `cargo check --locked -p fs --tests`                                                   | PASS    | Watcher cfg, FxHashMap, and macOS fast-path ports.                    |
| `cargo fmt --all -- --check`                                                           | PASS    |                                                                       |
| `git diff --check`                                                                     | PASS    |                                                                       |
| `cargo xtask check upstream-ledger`                                                    | PENDING | Ledger regenerated from this report.                                  |
| macOS-only paths (`24d3772111`, `d833d4689d`)                                          | NOT RUN | This host is Linux; the normalization fast path is not compiled here. |
