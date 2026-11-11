---
title: Upstream Sync 2026-11-11 (batch C)
description: Selective Zed upstream sync, the last commit through the 2026-11-11 live head.
---

# Upstream Sync 2026-11-11 (batch C)

## Scope

- Local base: `main` at `38ba31d7fb` (batch B recorded through `7203f3bdc3`)
- Target branch: `sync/upstream-2026-11-11`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `64ef8026c7bc3b29b53c57ebcd6e67a0d9a63ef0`
- Live upstream head queried: `43bd5d3cacadd6087c237fe2843f260595b933a7`
- Query time: `2026-11-11`
- Reviewed range: `64ef8026c7..43bd5d3cac`, 1 commit
- New reviewed baseline: `43bd5d3cacadd6087c237fe2843f260595b933a7`
- Remaining after this batch: zero; the range ends at the live head

The previous batch stopped before this commit because its read path needs a
`path_style` column plus a `recent_projects` shape ZZZ was believed to have
skipped. That turned out not to be the case: the bulk-delete refactor is
already absorbed (`ScrollBehavior` in `crates/picker`, `is_scrolled_to_end` in
both GPUI lists, `RecentProjectsDelegate::delete_recent_project`), and the free
`get_recent_projects` / `delete_recent_project` functions this commit removes
had no callers left. Nothing had to be absorbed first.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                                              |
| ---------- | ----- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 43bd5d3cac | B     | 6fd35b8c1b   | Persists the workspace `PathStyle` so recent projects and project groups render Windows remote paths with the remote's separators. Ported onto ZZZ's `SerializedWorkspace` and `ProjectGroupKey` shapes; payloads below. |

Totals: one `B`, no rejections, so the ledger is unchanged by this batch.

## Applied Work

- `6fd35b8c1b` is a `B` port of `43bd5d3cac`, signed off for DCO.

No remote, pull request, or named upstream remote was created.

## Per-Commit Narrative

### 43bd5d3cac — workspace `PathStyle` persistence (B)

The commit wires a path style through to the recent-projects list:

- `SerializedWorkspace` gains `path_style`, stored in a new `path_style TEXT`
  column on `workspaces` and serialised as `"unix"` / `"windows"`, with
  unrecognised or missing values falling back to the local style.
- `RecentWorkspace` gains `Option<PathStyle>`, so a row restored from the
  database can be rendered with the style it was saved under.
- `highlights_for_path` takes a `PathStyle` instead of using `Path::file_name`,
  which is what mis-split Windows remote paths on a Unix host.
- Recent projects and the sidebar resolve the style per project group: a loaded
  workspace's live `Workspace::path_style` first, the persisted value second,
  `PathStyle::local()` last.
- Each `paths`/`paths_order` pair is read through `PathListColumns`, because the
  extra column pushed the workspace queries past sqlez's ten-element tuple
  limit.

Local divergences the port had to respect:

- `PathStyle` has `Posix` and `Windows` locally, not upstream's `Unix`, so
  `Posix` serialises as `"unix"` (the on-disk value is unchanged from
  upstream) and the round-trip tests compare `PathStyle::Posix`.
- `MultiWorkspace::workspaces_for_project_group` returns
  `Option<Vec<Entity<Workspace>>>` rather than an iterator, so the new
  `path_style_for_project_group` goes through `.and_then` and `into_iter()`.
- `ProjectGroupKey::matches_identity` does not exist locally; the fallback uses
  `ProjectGroupKey::matches`, which is what the surrounding code already uses.
- ZZZ's `SerializedWorkspace` keeps `identity_paths`, and one local test built
  the struct without the new field, so it now sets `path_style:
PathStyle::local()`.
- The upstream test `test_native_window_state_round_trip` was not taken: ZZZ has
  no native window state. The `test_highlights_for_path` Windows case was
  restored with the port, since it is the only test covering the new parameter.

## Verification

| Check                                                    | Result  | Notes                                                                             |
| -------------------------------------------------------- | ------- | --------------------------------------------------------------------------------- |
| `cargo check --locked -p workspace -p recent_projects`   | PASS    |                                                                                   |
| `cargo check --locked -p zzz`                            | PASS    |                                                                                   |
| `cargo check --locked --workspace`                       | PASS    | Whole workspace.                                                                  |
| `cargo test --locked -p workspace --lib path_style`      | PASS    | `test_path_style_round_trip` and `test_missing_or_unknown_path_style`.            |
| `cargo test --locked -p recent_projects`                 | PASS    | 36 passed, including the bulk-delete scroll tests and `test_highlights_for_path`. |
| `cargo fmt --all -- --check`                             | PASS    |                                                                                   |
| `git diff --check`                                       | PASS    |                                                                                   |
| `cargo xtask check upstream-ledger`                      | PASS    | No new rejections; the ledger is unchanged.                                       |
| Windows remote paths rendered from a real remote session | NOT RUN | The path is covered by unit tests; this host is Linux and no remote was attached. |
