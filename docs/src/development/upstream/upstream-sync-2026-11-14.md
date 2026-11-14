# Upstream re-absorb run: 2026-11-14

## Scope

| Field                            | Value                                                                                                                 |
| -------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| Local base                       | `1e9a3c62de` (`build: bundle only the .app and quiet non-Linux warnings`)                                             |
| Upstream URL                     | `https://github.com/zed-industries/zed.git`                                                                           |
| Upstream ref                     | `refs/heads/main`                                                                                                     |
| Live upstream head at query time | `56a157e5b01b62afd5c50a014ed15b379de8dfd6` (2026-11-14T17:57:38Z)                                                     |
| Source of the candidate list     | GitHub issues #1-#40, opened from `upstream-sync-audit-2026-09-22.md` and `upstream-sync-audit-2026-10-18` audit rows |
| Candidates                       | 40                                                                                                                    |

This run is not a new upstream window. It re-checks the 40 commits that the
2026-09-22 and 2026-10-18 audits left behind as `C` rows or as unclassified
interval commits, and lands each one that the isolation test now passes. The
reviewed baseline (`aff99ecc32`) is unchanged: no new upstream range was
reviewed.

Five candidates were already on `main` before this run and needed no code
change: `d8664715` (`a0f9c9dafd`), `ef376006` (`0be71555de`),
`1dd9bbfd` (`c10e21c3e9`), `e4945b5d` (`70577d4138`), and the terminal
mouse-button mapping (`e6ea825f`, equivalent since `5be8c02a81`). Two more
were already equivalent without a local commit: `3de8ad9174` (the
context-menu highlight target already exists as `is_context_menu_target`
in `crates/git_graph/src/git_graph.rs`) and `40dc154a` (`ContextMenu` never
auto-selects on focus locally).

## Decisions

| Upstream | Class | Local commit | Disposition                                                                                    |
| -------- | ----- | ------------ | ---------------------------------------------------------------------------------------------- |
| a034d870 | B     | 517d21754f   | Worktree loading-entry cleanup runs in a detached task; the worktree scan rewrite was omitted. |
| d4010e91 | B     | 63363a1ed3   | Project-panel undo/redo confirms before trashing; prompts share `build_removal_prompt`.        |
| d8664715 | A     | a0f9c9dafd   | Already absorbed; CSV cell and header copy.                                                    |
| ef376006 | A     | 0be71555de   | Already absorbed; tab switcher file preview setting.                                           |
| ac09219c | B     | aeea7bb9cd   | Bottom dock clamps non-flexible panels on resize.                                              |
| e4671f71 | B     | a9d4293253   | Malformed global task files raise a notification.                                              |
| 625b29d2 | B     | c7d8307d35   | Refocusing project search selects the existing query.                                          |
| 40102ee7 | B     | 0a718c7319   | Inactive window bounds no longer overwrite saved bounds.                                       |
| 6d66d574 | B     | cb0e07eabf   | Git graph commit message is bounded to its scroll viewport.                                    |
| 948486f8 | B     | 360dd8b524   | Git panel context-menu focus is deferred one effect cycle.                                     |
| 2bb81ba8 | B     | ad6c3369e3   | Picker previews honor the line-number setting.                                                 |
| 1f99101e | B     | 24b126c176   | Blame gutter width includes the avatar's non-text width.                                       |
| 68b165e4 | B     | 039029e51e   | Project-panel undo is disabled on read-only projects.                                          |
| c9e8e611 | B     | e9776c8787   | LSP workspace folders carry names derived from their URIs.                                     |
| 2fd6e237 | B     | bf6331ef70   | File-finder worktree refreshes are coalesced.                                                  |
| 3f57e8d1 | B     | f202a4341a   | Center terminal startup does not steal modal focus.                                            |
| 8f6f8a54 | B     | fbb3eaecb0   | Active-pane decorations follow window focus.                                                   |
| 777aa2ee | B     | 9e9cc4b8e1   | Solo diff views expose their editor to buffer search.                                          |
| c5e0fb8f | B     | 035cdd27e4   | Soft-wrap toggles relay from the search bar to the searched editor.                            |
| 3de8ad91 | A     | —            | Already equivalent; `is_context_menu_target` in `crates/git_graph` keeps the row highlighted.  |
| 482963d5 | B     | 7eee42c9ef   | A single punctuation mark is its own word boundary.                                            |
| 9caf263a | B     | 6763264fe6   | Branch picker descriptions truncate.                                                           |
| 3c5e6198 | B     | e179f4eaa6   | Removing an open folder rekeys this window's recent-project group.                             |
| 547a6cd5 | B     | 5097cb0551   | Editor context menus scroll and snap to whole pixels.                                          |
| 2c4e4470 | B     | cc94f21e40   | Linux dispatcher leaks scheduled runnables during shutdown.                                    |
| dc2a339d | B     | 48e6ad47f7   | Wayland selection serials are tracked separately from other kinds.                             |
| 1dd9bbfd | A     | c10e21c3e9   | Already absorbed; `GlobalElementId` caches its path hash.                                      |
| e4945b5d | A     | 70577d4138   | Already absorbed; SSH transport reuses the platform style.                                     |
| e6ea825f | A     | 5be8c02a81   | Already equivalent; middle and right mouse buttons map correctly.                              |
| ced0d857 | B     | aa3fef7829   | Go highlight query marks builtins as `type.builtin`.                                           |
| 6a8f5a30 | B     | 5ca51ee90d   | Title-bar dropdown padding is stabilized.                                                      |
| 40dc154a | A     | —            | Already equivalent; local `ContextMenu` never auto-selects on focus.                           |
| 9902238f | B     | f81153f2ee   | Git reset triggers a project rescan; the fetch half was already present.                       |
| 94792bdb | B     | d04dfb2ff8   | Editorconfig cannot re-enable disabled trailing-whitespace removal.                            |
| 3ad8086e | B     | 40896cd691   | Seeded project searches escape the query in regex mode.                                        |
| edd90685 | B     | d9559ad925   | Hover popover markdown reflows soft breaks.                                                    |
| ac44e9f5 | B     | 3fa05761c6   | Stdio context servers restart when their project root changes.                                 |
| 79d238f6 | B     | f27770fdca   | Shift+drag starts a terminal selection under mouse tracking.                                   |
| b76e4db1 | B     | 4ccad78973   | GnuPG signing prompts route through the askpass UI.                                            |
| 502cbccb | B     | dc609b7ee6   | Gutter hover buttons and menus require a worktree-backed buffer.                               |

## Applied work

Every class `B` row is a port onto current ZZZ APIs, committed with
`Upstream:` / `Retained:` / `Omitted:` trailers. Notable omissions:

- `a034d870`: the `crates/worktree` `RemovedEntries` generation rewrite and
  the upstream `Window`-shaped test adjustments were left out; only the
  loading-entry leak fix was kept, and its second regression test asserts
  `initial_scan_completed()` because the upstream `diagnostics()` accessor
  does not exist locally.
- `d4010e91`: the prompt strings reuse the existing `project_panel.delete.*`
  keys, so no locale keys were added.
- `2c4e4470`: the `gpui_windows` half was not ported (no Windows toolchain on
  this host).
- `9caf263a`: the per-section remote/local header separators the upstream
  commit also touches do not exist in the local branch picker.
- `9902238f`: the fetch half was already implemented locally; only the
  `reset` path gained a rescan.
- `68b165e4`: the upstream read-only-guest test cannot be ported (`Project`
  has no collab role or capability override locally), so only the writable
  guard is covered.
- `2bb81ba8`: the upstream regression test was dropped rather than add an
  unused `Editor::show_line_numbers` accessor.

Two commits in this run are not ports:

- `996476086d` `style: format the crates touched by the upstream re-absorb`
  settles the rustfmt drift the ports introduced.
- `d04dfb2ff8` carries the adapted tests for `94792bdb`.

## Verification

| Check                                                                                                                                                                                                                                                                                      | Status                                                             |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------ |
| `cargo fmt --all -- --check`                                                                                                                                                                                                                                                               | PASS (after `996476086d`)                                          |
| `cargo xtask clippy` (philosophy preflight, workspace clippy `--all-features --all-targets --deny warnings`, `future_not_send`, `cargo-machete`, `typos`)                                                                                                                                  | PASS (after `0899acd714` and `7fb7c31f22`)                         |
| `cargo check --locked` on `project`, `project_panel`, `workspace`, `search`, `git_ui`, `git_graph`, `editor`, `language`, `lsp`, `file_finder`, `terminal`, `terminal_view`, `recent_projects`, `grammars`, `picker`, `settings_json`, `title_bar`, `gpui`, `gpui_linux`, `askpass`, `git` | PASS                                                               |
| `cargo test -p project_panel trash`, `-p project_panel dirty_file`, `-p project_panel undo`                                                                                                                                                                                                | PASS                                                               |
| `cargo test -p workspace` (256), `-p search`, `-p file_finder` (83), `-p terminal` (82), `-p terminal_view` (72), `-p recent_projects` (39), `-p grammars` (68), `-p settings_json` (9), `-p gpui_linux` (38), `-p askpass` (4), `-p lsp` (14)                                             | PASS                                                               |
| `cargo test -p editor blame`, `movement`, `test_mouse_context_menu`, `hover_markdown`, `gutter`, `test_markdown_save_respects_disabled_trailing_whitespace_removal`                                                                                                                        | PASS                                                               |
| `cargo test -p project --test integration context_server` (21), `test_editorconfig_support`                                                                                                                                                                                                | PASS                                                               |
| `cargo test -p project git` (21 + 43)                                                                                                                                                                                                                                                      | PASS                                                               |
| `cargo xtask check locale-keys` (4043 keys)                                                                                                                                                                                                                                                | PASS                                                               |
| Wayland compositor behavior, signed-commit prompt flow, Windows dispatcher                                                                                                                                                                                                                 | NOT RUN (no compositor, passphrase-protected key, or Windows host) |
| Manual `git reset`/`fetch` panel refresh                                                                                                                                                                                                                                                   | NOT RUN                                                            |

All 40 issues (#1-#40) were closed against these commits.
