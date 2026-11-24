---
title: Upstream Sync 2026-11-20 (B)
description: Selective Zed upstream sync, the 17 commits after the 2026-11-20 batch A.
---

# Upstream Sync 2026-11-20 (B)

## Scope

- Local base: `sync/upstream-2026-11-20` at `c47242f8aa`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `f88e54eed5474cab7504835e60b0c0d7d6e39485`
- Reviewed range: `f88e54eed5..0c5af8c563`, 17 commits, oldest first
- New reviewed baseline: `0c5af8c5634df7069a7b51794920e80128bf1081`
- Remaining after this batch: 27 commits, starting with `36acea54c5`

This batch is dominated by the two dead-code sweeps and by commits that
target surfaces local does not carry. Eight commits port onto the local
API shape and nine are rejected, seven of them because the code they
touch never entered the tree: the in-tree model selector and provider
crates were deleted by the ACP-only cleanup, the Snap and installer
release plumbing and the Linux sandbox live only in the upstream
distribution, and `piper` is already at the version upstream bumps to.

Two rejections are not about absent files. `5cffb90247` is a follow-up to
`bc29bcfe72` (2026-07-07), which batches diff-base loads through
`GitRepository::load_revisions`; that predecessor is an ancestor of the
previous baseline but appears in no report and the ledger, so the
follow-up has no local pre-image. `4a4c8150f0` bumps the pinned git
tree-sitter revision, while local tracks the published `tree-sitter`
crate at 0.27.2.

The batch ends early at `0c5af8c563`. The next commit, `36acea54c5`,
removes dead code across the language, markdown, and text crates and
conflicts in twelve files where local diverged deliberately — the
`attr("class")` helper, `RowChunkList::previous_chunk`, and the
`tree-sitter-gitcommit` grammar are local rewrites, not upstream code —
so it needs a pass of its own rather than a mechanical resolution.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                                     |
| ---------- | ----- | ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1043f4a86a | C     | --           | Adds a commit-message model picker on a new `language_model_selector` crate and `agent_ui::agent_model_selector`. The ACP-only cleanup removed both, so there is no selector to route a pick through.           |
| 621a6adf30 | B     | 7f5f22a9cb   | Rendered lines map a source index that lands inside substituted text to a character boundary. Payload below.                                                                                                    |
| 9f1c56d6f2 | B     | 313c025ca3   | An empty content mask stays empty when snapped to device pixels. Payload below.                                                                                                                                 |
| b840e80bf2 | B     | f8aa0fa398   | Bumps the app version to 1.31.0. Payload below.                                                                                                                                                                 |
| 8c514d28ed | C     | --           | Adds the official Snap packaging and the release workflows that build it. Local has no `script/snap-build` and no `release_nightly`/`after_release` xtask task, so the commit's own files do not exist.         |
| 5cffb90247 | C     | --           | Drains git's stderr instead of piping it while loading many diff bases. The `load_revisions` it modifies came from `bc29bcfe72`, which was never absorbed, so the hunk has no local pre-image.                  |
| 264f0b67be | C     | --           | Bumps `piper` to 0.2.5. Local `Cargo.lock` already pins 0.2.5 with the same checksum, so the commit is already in effect.                                                                                       |
| 988007a89a | C     | --           | Updates the ACP SDK to 5.4.0 and aligns the `agent_servers::acp` v1/v2 split. Local pins `=5.3.0` and keeps a single `acp.rs` with no session-update module, so the targets do not exist.                       |
| 846ff80e0e | B     | 48b9e9baf6   | `*` and `#` select the word under the cursor with Vim's character-kind rules. Payload below.                                                                                                                    |
| dd091a1b4f | C     | --           | Replaces Zed's installer terms with the General Terms and Zed Product Terms. Local keeps its own `legal/terms.md` and has no installer or `script/generate-terms-rtf`, so the documents it edits are not local. |
| 3c2118dd69 | C     | --           | Corrects hosted model pricing and availability. Local ships no hosted-model catalog; the surface is the one the ACP-only cleanup removed.                                                                       |
| 2eff23878e | B     | c12cb3a6b8   | Normalizes clipboard line endings on Windows. Payload below.                                                                                                                                                    |
| 4a4c8150f0 | C     | --           | Bumps the pinned `tree-sitter` git revision. Local depends on the published `tree-sitter` 0.27.2 instead of the git revision, so the pin it edits has no local counterpart.                                     |
| 4e161e70b5 | C     | --           | Adds an Extra High thinking level for Gemini in `crates/google_ai`. Local has no `google_ai` crate.                                                                                                             |
| 740e018b70 | B     | 2ea78012eb   | A private file stays private when opened over a remote project. Payload below.                                                                                                                                  |
| b51ffab015 | B     | 695ac4da7c   | The text finder clears its previous results when a search returns nothing. Payload below.                                                                                                                       |
| 0c5af8c563 | B     | 7af41063ba   | Removes dead code and redundant notifications in the git, debugger, and panel UIs. Payload below.                                                                                                               |

Totals: zero `A`, eight `B`, nine `C`.

## Applied Work

- Every landed commit is a `B` port and carries `git cherry-pick -s`
  sign-off for DCO.
- The version bump follows the upstream app crate rather than copying it:
  local's app crate is `crates/zzz`, so the manifest and lockfile entry
  carry 1.31.0 and `crates/zed` stays absent.

No remote, pull request, or named upstream remote was created.

## Per-Commit Narrative

### 621a6adf30 — markdown selections on character boundaries (B)

Retained: `push_text` records a mapping per character when the rendered
text is longer than its source, such as dashes substituted for a run of
hyphens or a smart quote replacing a straight one. Both mapping
directions keep their result inside the segment that contains the index,
and `RenderedText::text_for_range` floors both ends to a character
boundary. A selection that outlives the source it was made in no longer
panics while the conversation view streams.

Omitted: upstream's
`test_selection_query_after_streaming_hides_clicked_syntax`, which needs
`add_to_workspace_with_size` and direct `acp_v1` streaming that the local
test module does not carry. The mapping tests ported here fail without
the fix: the dash mapping case and the `- foo` list-marker case.

### 9f1c56d6f2 — empty content masks stay empty (B)

Retained: `Window::cover_bounds` returns a zero-size region when its
bounds are empty, instead of rounding outward into a one-pixel strip at
fractional positions, and folds negative sizes into zero because the
blurred-shadow shader treats them the same.

Omitted: nothing. Local moved this function out of the monolithic
`window.rs` into `crates/gpui/src/window/methods.rs`, so the guard was
applied there. Upstream's four paint-at-scale tests became two direct
`cover_bounds` tests covering the empty and fractional-edge cases, since
the paint helpers they use are local-only.

### b840e80bf2 — app version 1.31.0 (B)

Retained: the fork keeps tracking Zed releases, so the app version moves
to 1.31.0 in the manifest and the lockfile entry.

Omitted: nothing; upstream's file is `crates/zed/Cargo.toml`, local's is
`crates/zzz/Cargo.toml`.

### 846ff80e0e — Vim's word selection for `*` and `#` (B)

Retained: `*` and `#` select the word under the cursor using the same
character-kind classification Vim uses, so a run of punctuation or a mix
of word and non-word characters selects the way Vim does, and the
visual-mode variants extend the selection the same way.

Omitted: nothing. The only conflict was the import line: local has no
`gpui::TaskExt`, and the ported code only needs
`Task::detach_and_log_err`, which is inherent on the local task type.

### 2eff23878e — clipboard line endings on Windows (B)

Retained: `write_string` rewrites LF to CRLF before handing
CF_UNICODETEXT to the OS, because Windows applications expect CRLF, and
`read_string` folds CRLF back to LF so callers see one convention on
every platform. `read_raw_clipboard_text` exposes the untranslated text
for the regression test.

Omitted: nothing. The local write path keeps its CF_HTML and metadata
handling, so only the text branch was replaced, and the local test module
already had the surrounding credential tests the upstream pre-image
expects. Not compiled: `gpui_windows` builds only on Windows.

### 740e018b70 — private files over remote projects (B)

Retained: the worktree `File` message carries `is_private`, so a value
opened over a remote connection keeps the server's private-file verdict
and editor redaction applies to it. The client previously defaulted the
flag to `false` and displayed the plaintext.

Omitted: the `IndexTextFile` hunk, because local has no `IndexTextFile`;
its `language::File` implementation does not exist to carry the flag. The
local `Archive` file reports `false`, matching its existing `is_private`
accessor. The proto field takes tag 9, since local already uses 7 and 8
for `vfs_path` and `resource_id`.

### b51ffab015 — text finder clears on no results (B)

Retained: `stream_results_to_picker` clears the picker after its loop
when nothing arrived and the search was not cancelled, so an empty result
set no longer leaves the previous query's matches on screen. The three
duplicated reset blocks collapse into `Delegate::clear_results`.

Omitted: `adjust_query_regex_language`, which the upstream pre-image
carries beside the new helper and which local does not have; this commit
does not call it. `clear_results` resets the local `collapsed_files`
field instead of upstream's `unique_files` and `collapsed_paths` pair.

### 0c5af8c563 — dead code in git and panel UIs (B)

Retained: drops the `cx.notify()` calls that follow helpers which already
notify, folds `visible_worktrees().collect().len()` into `count()`, and
removes unused items — `Branch::tracking_status`, `Branch::priority_key`,
`Upstream::stripped_ref_name`, `ProjectDiff::deploy_at_project_path`,
`RepositorySelectorDelegate::update_repository_entries`, and
`git_picker`'s `open_branches`/`open_stash` wrappers.

Omitted: the parts of the upstream pre-image local never carried. There
is no `call`-gated `potential_co_authors` or `local_committer` and no
`show_commit_message_error`, so those hunks resolve to nothing, and the
`git_graph` struct change lands in `crates/git_graph`, where local keeps
that code, instead of upstream's `crates/git_ui/src/git_graph.rs`. Local
still needs `use git;` in `project_panel` for `git::RestoreFile`, so its
unused `use fs::TrashId;` is dropped instead.

## Verification

| Check                                                                | Result  | Notes                                 |
| -------------------------------------------------------------------- | ------- | ------------------------------------- |
| `git status --porcelain` between commits                             | PASS    | clean worktree                        |
| `cargo test -p markdown --lib test_mappings`                         | PASS    | 1 passed                              |
| `cargo test -p markdown --lib test_surrounding_word_range`           | PASS    | 2 passed                              |
| `cargo test -p gpui --lib content_mask`                              | PASS    | 2 passed                              |
| `cargo xtask check release`                                          | PASS    | after the 1.31.0 bump                 |
| `cargo test -p remote_server --lib test_remote_private_file`         | PASS    | 2 passed                              |
| `cargo check -p worktree -p proto`                                   | PASS    | proto `is_private` field              |
| `cargo test -p search --lib text_finder`                             | PASS    | 8 passed                              |
| `cargo test -p project_panel --lib test_editing_files`               | PASS    | 1 passed                              |
| `cargo test -p git_ui --lib`                                         | PASS    | 132 passed                            |
| `cargo check -p project_panel -p git_ui -p git_graph -p debugger_ui` | PASS    | dead-code port                        |
| `cargo fmt --all -- --check`                                         | PASS    |                                       |
| `git diff --check`                                                   | PASS    |                                       |
| `cargo xtask check philosophy`                                       | PASS    | no rejected surface returned          |
| `cargo check -p gpui_windows` on Windows                             | NOT RUN | `gpui_windows` builds only on Windows |
| macOS / Windows runtime behaviour                                    | NOT RUN | host is Linux                         |
