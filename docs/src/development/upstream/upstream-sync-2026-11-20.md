---
title: Upstream Sync 2026-11-20
description: Selective Zed upstream sync, the first 20 of 64 commits after the 2026-11-17 batch C.
---

# Upstream Sync 2026-11-20

## Scope

- Local base: `main` at `34bf0eb6ed`
- Target branch: `sync/upstream-2026-11-20`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `698368515d17a602dd51ea61297e5412ce06f52f`
- Live upstream head queried: `34639504475cef1677149ad351b3788a89dba865`
- Query time: `2026-11-20`
- Reviewed range: `698368515d..f88e54eed5`, the first 20 commits, oldest first
- New reviewed baseline: `f88e54eed5474cab7504835e60b0c0d7d6e39485`
- Remaining after this batch: 44 commits, starting with `1043f4a86a` and
  `621a6adf30`

The batch is mostly narrow correctness and performance work in `git`,
`vim`, `project`, `worktree`, `editor`, `multi_buffer`, and `git_ui`. Two
commits conflict with locally diverged code but remain portable: the Helix
clipboard commit touches a test block whose order changed locally, and the
worktree commit deletes a path-construction block that local writes
differently. One port needed a local shape for the LSP trigger lookup
because the local `LocalLspStore` keeps no initial-capability snapshot, and
one merges local tests that the upstream patch presupposes.

The five rejections are four absent surfaces and one unused test API: the
edit-prediction CLI and the in-tree language-model providers were deleted
by the ACP-only cleanup, the collaboration dead-code sweep covers crates
local does not ship, the git_ui commit view has no reveal-file anchor to
position, and the new `HeadlessAppContext` helper has no local caller.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                                                                                                              |
| ---------- | ----- | ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| b525067c80 | A     | f16d77513f   | Replaces `size_full` with `flex_1` for the commit modal's editor container, removing the empty strip under the message. Cherry-picked intact.                                                                                                                                            |
| 58849026ec | B     | 84965464d8   | Helix yank, delete, and paste consult `vim.use_system_clipboard` instead of forcing `Never`. Payload below.                                                                                                                                                                              |
| a701d985a8 | B     | d9a5bb9757   | Each completion request is checked against the triggering server's own trigger characters. Payload below.                                                                                                                                                                                |
| 4122f44f6f | B     | 27aa5899a0   | `BackgroundScanner::scan_dir` matches ancestor paths once per directory instead of once per child. Payload below.                                                                                                                                                                        |
| df5cf7b718 | A     | 435d0d371e   | Adds `editor: fold at current indentation level`, deriving the fold level from the cursor line. Applied intact with its eight tests.                                                                                                                                                     |
| 58be7923cf | B     | f65f8449c0   | Reports the git blame stdin write error only after the child's exit status, so a large untracked file yields empty blame instead of `Broken pipe`. Payload below.                                                                                                                        |
| 96362135eb | B     | a2b84bde82   | `RelPath::cmp` compares bytes with `/` ranked below every other byte instead of building component iterators. Payload below.                                                                                                                                                             |
| bb5e6dfd07 | B     | 8f8a8a5b61   | A whitespace-only name reports the empty-name error instead of the whitespace warning. Payload below.                                                                                                                                                                                    |
| 5b282ac8bb | C     | --           | Adds `HeadlessAppContext::simulate_window_scale_factor_change`. Local `TestAppContext` already carries `simulate_window_scale_factor_change`, and no local test needs the headless variant, so the public helper would have no caller.                                                   |
| 705b429e78 | B     | c163671135   | `MultiBuffer::sync` resyncs only the buffers that changed, and buffer statuses are counted incrementally. Payload below.                                                                                                                                                                 |
| 3fd0253081 | B     | 52af308191   | The Follow toggle applies on every toggle instead of only while the thread is generating. Payload below.                                                                                                                                                                                 |
| 1ab8b9abfb | C     | --           | Fixes total token percentiles in `crates/edit_prediction_cli/src/score.rs`. Local `crates/` has no `edit_prediction_cli`; the evaluation CLI was never part of ZZZ.                                                                                                                      |
| 881c618a6a | C     | --           | Removes dead code across `crates/client`, `crates/call`, `crates/channel`, `crates/collab`, `crates/livekit_api`, and `crates/rpc`. Local ships none of `call`, `channel`, `collab`, or `livekit_api`, so the commit targets the rejected collaboration surface.                         |
| f070c7ff6c | A     | 6a18279c6d   | Makes `SettingsFile::Global` compare equal to itself so `BTreeMap` keys behave. Applied intact with the ordering assertions.                                                                                                                                                             |
| 761786a4ef | B     | eec12de04f   | Clears the `loading_diffs` entry after a failed load so the next `open_diff_since` retries. Payload below.                                                                                                                                                                               |
| 38f5c4af0d | C     | --           | Folds commit-view buffers as soon as they are inserted and anchors the reveal scroll to `Block::FoldedBuffer`. Local `commit_view.rs` has no reveal-file option, no `add_commit_file` helper, and no `Block::BufferHeader` scroll anchor, so the pre-image does not exist.               |
| e944cfa2fe | B     | 402c88f7c2   | Documents the system clipboard register in Helix mode. Payload below.                                                                                                                                                                                                                    |
| da294762d6 | B     | 0d7c21bae2   | Converts the shared scroll anchor to each side's native anchor before a split teardown. Payload below.                                                                                                                                                                                   |
| f3626706fa | C     | --           | Adds a typed OpenAI Ultrafast service tier across `crates/open_ai`, `crates/anthropic`, `crates/language_models`, `crates/language_models_cloud`, and `crates/openai_subscribed`. The ACP-only cleanup deleted every in-tree provider and hosted-model crate, so no local caller exists. |
| f88e54eed5 | B     | 063fae7d62   | Tool-call locations resolve to `Option<AgentLocation>` so non-text files still open. Payload below.                                                                                                                                                                                      |

Totals: three `A`, twelve `B`, five `C`.

## Applied Work

- `b525067c80`, `df5cf7b718`, and `f070c7ff6c` are direct `A` and used
  `git cherry-pick -x -s`.
- `58849026ec`, `a701d985a8`, `4122f44f6f`, `58be7923cf`, `96362135eb`,
  `bb5e6dfd07`, `705b429e78`, `3fd0253081`, `761786a4ef`,
  `e944cfa2fe`, `da294762d6`, and `f88e54eed5` are `B` ports, signed off
  for DCO.

No remote, pull request, or named upstream remote was created.

## Per-Commit Narrative

### 58849026ec — Helix honors `use_system_clipboard` (B)

Retained: `VimGlobals` reads `VimSettings::get_global(cx).use_system_clipboard`
for default-register writes and reads instead of routing through
`default_register_clipboard_policy`, which forced `Never` in Helix mode.
`VimTestContext::set_use_system_clipboard` sets the setting per test, and
the yank and paste tests split into default and `Never` cases.

Omitted: nothing. The conflict was test ordering: local places
`test_replace_with_crlf` after `test_helix_yank`, while upstream places it
before, so the rename to `test_helix_yank_default` had to be resolved by
hand without duplicating the replace test.

### a701d985a8 — completion context per target server (B)

Retained: before a completion request is sent, `LspStore` checks the
trigger character against the trigger characters registered by that
server. An unregistered character becomes `INVOKED` with no trigger
character, so Emmet's letter triggers no longer suppress Vue prop
completions.

Omitted: upstream's `completion_trigger_characters_for_buffer`, which reads
`initial_server_capabilities` and the dynamic registration map. Local
`LocalLspStore` keeps no initial-capability snapshot and reads
`language_server.capabilities()` live, but `Buffer` already tracks
per-server triggers, so the port adds
`Buffer::completion_triggers_for_server` and reads that instead. The new
integration test `test_completion_context_uses_target_server_triggers`
covers both static and dynamic registration; upstream's
`test_completion_context_survives_stale_trigger_replay` was omitted because
it depends on the dynamic-unregistration helpers that local does not have.

### 4122f44f6f — ancestor checks once per scanned directory (B)

Retained: `BackgroundScanner::scan_dir` evaluates its own path for private
and hidden state once, then tests only each child's own path for
`file_scan_exclusions`, `private_files`, and `hidden_files`, so ancestor
matching no longer reruns for every entry on initial scans and rescans.

Omitted: nothing. The conflict was the local `RelPath::unix(..).expect(..)`
path construction inside the replaced block, which the port deletes.

### 58be7923cf — git blame write error after exit status (B)

Retained: `run_git_blame` joins the stdin write as a value, checks the
child's exit status first, and surfaces the write error only when git
exited successfully. Blaming a large untracked file returns empty entries
instead of `Broken pipe`.

Omitted: nothing. Local `run_git_blame` always writes to a piped stdin and
has no `BlameSource`/`Option<ChildStdin>` shape, so the final `flush`
needed an explicit `anyhow::Error::from` to keep the wrapped future's
error type inferable; without it `cargo check -p git` fails with `E0282`.

### 96362135eb — RelPath comparison without components (B)

Retained: `RelPath::cmp` compares the raw bytes, ranking `/` below every
other byte and skipping 16-byte equal chunks first, so ordering still
matches component order without building iterators per comparison.

Omitted: nothing. Upstream keeps the type in `crates/path`; local keeps it
in `crates/util/src/rel_path.rs`, so the patch and its regression test were
applied there.

### bb5e6dfd07 — whitespace-only name error (B)

Retained: `ProjectPanel::populate_validation_error` trims the name before
the emptiness check, so a whitespace-only name reports the empty-name
error and refuses the confirm.

Omitted: nothing; the product hunk was adapted to the local `tr(..)`
catalog calls, and the test asserts the error and warning states after a
wait for the editor change to reach the panel.

### 705b429e78 — sync only changed buffers (B)

Retained: `Buffer::record_changes` registers a shared `HashSet<BufferId>`
instead of a change bit, so `MultiBuffer::sync` and `sync_mut` resync only
the buffers that changed or were added. `BufferStateSnapshots` keeps dirty,
deleted-file, and conflict counts incrementally, replacing the three
booleans on `MultiBufferSnapshot`.

Omitted: upstream's new `multi_buffer_tests` additions and their helpers.
The local test file already defines those helpers (`mutate_excerpt_ranges`,
`check_multibuffer`, `check_multibuffer_edits`, `format_diff`,
`assert_excerpts_match`) at different positions with local-only tests, so
the upstream block collides instead of applying. The production change is
covered by the existing 59 `multi_buffer` tests, which pass.

### 3fd0253081 — Follow toggle after an idle disable (B)

Retained: `ThreadView::toggle_following` applies the workspace follow or
unfollow on every toggle, so a toggle made after the thread went idle still
takes effect and keeps `should_be_following` in step with the workspace.

Omitted: upstream's `telemetry::event!("Follow Agent Selected", ..)`; local
builds carry no telemetry surface.

### 761786a4ef — retry a diff-since load (B)

Retained: `GitStore::open_diff_since` removes its `loading_diffs` entry
after the future completes rather than inside the success branch, so a
failed blob read no longer caches the failure for every later call.

Omitted: nothing. The local function differs from upstream's staged-diff
helper, so the hunk was applied by hand, and the test was ported with
`state.oids` as the local `HashMap<Oid, String>`.

### e944cfa2fe — Helix clipboard documentation (B)

Retained: `docs/src/helix.md` explains that Helix mode uses the system
clipboard by default, how to switch to internal registers with
`vim.use_system_clipboard`, and that `+` still addresses the system
clipboard explicitly.

Omitted: nothing; the paragraph was rebranded from Zed to ZZZ to match the
local docs.

### da294762d6 — scroll anchors across a split teardown (B)

Retained: `SplittableEditor::unsplit` and a new `on_release` handler both
run `unshare_scroll_anchors`, which converts the shared scroll anchor to
each side's native anchor before split state is torn down. Moving a project
with a split diff to a new window no longer converts an anchor against a
released display map.

Omitted: the `observe_global_in::<SettingsStore>` block that upstream's
pre-image carries in the same subscription list. Local never absorbed that
earlier commit, and including it here would add unrelated split toggling on
diff-view-style changes.

### f88e54eed5 — tool-call locations without a resolved buffer (B)

Retained: `AgentThreadEntry::location` returns
`Option<(ToolCallLocation, Option<AgentLocation>)>`, so a tool call that
points at a file the agent cannot resolve to a buffer still exposes its
path and line, and `ThreadView::open_tool_call_location` opens that path and
only moves the selection when a resolved location exists.

Omitted: upstream's `test_open_tool_call_location_without_resolved_buffer`.
Local `thread_view.rs` has no `mod tests`, and its helpers live in
`conversation_view::tests`, which cannot reach the private
`open_tool_call_location`. The `acp_thread` test
`test_tool_call_location_resolves_external_file` asserts the new
`Option<AgentLocation>` shape instead.

## Verification

| Check                                                                                          | Result  | Notes                                               |
| ---------------------------------------------------------------------------------------------- | ------- | --------------------------------------------------- |
| `git status --porcelain` before the batch                                                      | PASS    | clean worktree                                      |
| `git ls-remote` and `git fetch --no-tags` against the upstream URL                             | PASS    | live head `3463950447`, `698368515d` is an ancestor |
| `cargo check --locked -p vim --tests`                                                          | PASS    | Helix clipboard port                                |
| `cargo test -p project --test integration test_completion_context_uses_target_server_triggers` | PASS    | 1 passed                                            |
| `cargo check --locked -p worktree`                                                             | PASS    | ancestor-path scan port                             |
| `cargo check --locked -p editor --tests`                                                       | PASS    | fold-at-indentation commit                          |
| `cargo test -p git --lib test_blame_untracked_path_with_large_contents`                        | PASS    | 1 passed                                            |
| `cargo test -p util --lib rel_path`                                                            | PASS    | 29 passed, including the new component-order test   |
| `cargo test -p project_panel --lib test_editing_files`                                         | PASS    | 1 passed                                            |
| `cargo test -p multi_buffer`                                                                   | PASS    | 59 passed                                           |
| `cargo check --locked -p agent_ui`                                                             | PASS    | Follow toggle and tool-call location ports          |
| `cargo test -p settings --lib test_file_ord`                                                   | PASS    | 1 passed                                            |
| `cargo test -p project --lib test_open_diff_since_retries_after_failed_load`                   | PASS    | 1 passed                                            |
| `cargo test -p editor --lib test_rhs_editor_outlives_released_split`                           | PASS    | 1 passed                                            |
| `cargo test -p acp_thread --lib test_tool_call_location_resolves_external_file`                | PASS    | 1 passed                                            |
| Prettier on `docs/src/helix.md`                                                                | PASS    | `prettier@3.9.10 --write` reported unchanged        |
| macOS / Windows runtime behaviour                                                              | NOT RUN | host is Linux                                       |
