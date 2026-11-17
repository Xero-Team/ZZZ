---
title: Upstream Sync 2026-11-17 (batch B)
description: Selective Zed upstream sync, the remaining six commits after the 2026-11-17 batch.
---

# Upstream Sync 2026-11-17 (batch B)

## Scope

- Local base: `sync/upstream-2026-11-17` after `300b23e72a`; `main` is
  `afe8038860`
- Target branch: `sync/upstream-2026-11-17`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `d833d4689d04183d1dfd1ad862dc80f6f044a41f`
- Live upstream head queried: `698368515d17a602dd51ea61297e5412ce06f52f`
- Query time: `2026-11-17`
- Reviewed range: `d833d4689d..e5be9608dd`, six commits, oldest first
- New reviewed baseline: `e5be9608ddd4cb0f2f179e1c0014f0a3a0337260`
- Remaining after this batch: commits added upstream after `e5be9608dd`, up to
  the live head `698368515d`

This closes the window that the 2026-11-17 batch left open. Half the batch is
portable: the web keystroke modifier fix, the permission-dropdown pattern
selection, and the bare-repository check. The three rejections are absent local
surfaces: no `corgi.toml`, no `crates/git_ui/src/git_graph.rs`, and no
`read_file_tool.rs` in the agent crate.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                                                                                            |
| ---------- | ----- | ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 9925feb84c | B     | eaffcbdc86   | Drops Shift from single-character symbol keystrokes on the web platform, so `!` matches instead of `shift-!`. Payload below.                                                                                                                                           |
| 5709f329dc | C     | --           | Adds Corgi build configuration for FHS Linux to `corgi.toml` and `nix/dev`. ZZZ has the `nix/` tree but no `corgi.toml` at the repo root, so there is nothing to extend. Upstream-only build tooling.                                                                  |
| 4191aeaf50 | B     | 3fbbc2fd8b   | Checks only the pattern the user clicked in the permission dropdown. Payload below.                                                                                                                                                                                    |
| 482e76524b | C     | --           | Passes the selected file to `CommitViewOptions::reveal_file` from `LogSource::Path` in `crates/git_ui/src/git_graph.rs`. Local `crates/git_ui/src` has no `git_graph` module, so the call site and its `LogSource` type are absent.                                    |
| 6e7c5d3ae8 | C     | --           | Accepts stringified `start_line`/`end_line` in `crates/agent/src/tools/read_file_tool.rs`. Local `crates/agent/src/tools/` holds only `context_server_registry.rs`, and `deserialize_maybe_stringified` does not exist, so the tool and its schema have no local home. |
| e5be9608dd | B     | 403aa7361f   | Requires `HEAD` and `config` to be files before treating a directory as a bare repository. Payload below.                                                                                                                                                              |

Totals: zero `A`, three `B`, three `C`.

## Applied Work

- `9925feb84c`, `4191aeaf50`, and `e5be9608dd` are `B` ports, signed off for
  DCO.

No remote, pull request, or named upstream remote was created.

## Per-Commit Narrative

### 9925feb84c — shifted symbols on the web platform (B)

Retained: both the keydown and keyup handlers build their `Keystroke` through
`keystroke_modifiers`, which clears Shift for a single-character key whose
lowercase and uppercase forms are identical. Browsers report the shifted
character as the key, so `Shift+1` arrives as `!`; keeping the modifier made it
`shift-!`, which never matched a `!` binding. Shift stays on letters, where it
distinguishes case, and on named keys such as `tab` and `enter`, matching the
Linux platform's rule.

Omitted: the `browser_pointer_id_reuse_gets_a_new_touch_id` test that upstream's
import block brought along. Local `crates/gpui_web/src/events.rs` has no
`mod tests` and no `TouchIds` type, so only `shift_is_dropped_only_from_symbol_keys`
was added, in a new test module.

### 4191aeaf50 — permission dropdown pattern selection (B)

Retained: the first click on a pattern row checks only that row instead of
checking every row, later clicks toggle only their own row, and unchecking the
last checked row drops the selection so the dropdown falls back to its default
choice. An index with no matching row no longer enters pattern mode.

Omitted: upstream's `PermissionRequest::legacy_options` accessor. Local
`Conversation::toggle_permission_pattern` destructures `request.options`
directly, so the same guard is expressed with `pattern_index >= patterns.len()`.
`PermissionSelection::has_any_checked_patterns` became `pub(super)` and the
regression test was written against the local `StubAgentConnection` permission
fixture rather than upstream's `request_test_tool_authorization_with_options`
helper, which local does not have.

### e5be9608dd — bare repository detection (B)

Retained: `is_dot_git` only reports a bare repository when `config` is a file
and `HEAD` is a file or a symlink. A source directory that contains `head/` and
`config/` is no longer treated as a repository root, including on
case-insensitive filesystems where `head` matches `HEAD`. A dangling `HEAD`
symlink still counts, because Git resolves the ref by name through
`core.preferSymlinkRefs` and `packed-refs`, while `config` must resolve to a
real file.

Omitted: the upstream unit test for `is_dot_git`. Local `crates/worktree` sets
`[lib] test = false`, so its `mod tests` never runs and a private-function test
has nowhere to go. The `test_open_gitignored_files` integration test was
updated as upstream did, which covers the call-count change the new
implementation causes.

## Verification

| Check                                                                                        | Result  | Notes                                                                                                                       |
| -------------------------------------------------------------------------------------------- | ------- | --------------------------------------------------------------------------------------------------------------------------- |
| `cargo check --locked -p gpui_web --target wasm32-unknown-unknown --tests`                   | BLOCKED | Fails on `wasm_thread` needing a nightly feature, before reaching `gpui_web`. Baseline limitation, not caused by this port. |
| `RUSTC_BOOTSTRAP=1 cargo check --locked -p gpui_web --target wasm32-unknown-unknown --tests` | PASS    | Compiles the keystroke port and its test.                                                                                   |
| `cargo test --locked -p agent_ui test_toggle_command_pattern_toggles_only_clicked_pattern`   | PASS    | 1 passed, the ported regression test.                                                                                       |
| `cargo test --locked -p agent_ui test_tool_permission_buttons_terminal_with_pattern`         | PASS    | 1 passed.                                                                                                                   |
| `cargo test --locked -p agent_ui test_allow_button_uses_selected_granularity`                | PASS    | 1 passed.                                                                                                                   |
| `cargo check --locked -p worktree`                                                           | PASS    | Bare-repository port.                                                                                                       |
| `cargo test --locked -p worktree --test integration`                                         | PASS    | 104 passed.                                                                                                                 |
| `cargo fmt --all -- --check`                                                                 | PASS    | After the ports.                                                                                                            |
| `git diff --check`                                                                           | PENDING |                                                                                                                             |
| `cargo xtask check upstream-ledger`                                                          | PENDING | Ledger regenerated from this report.                                                                                        |
| Browser runtime behaviour of the shift change                                                | NOT RUN | This host is Linux; the wasm target only type-checks here.                                                                  |
