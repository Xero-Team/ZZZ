---
title: Upstream Sync 2026-09-24
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-09-24

## Scope

- Target branch: `sync/upstream-2026-09-24` from local `main` at
  `8c9344c324f9d9e11523a9cdbf367ad356bfe8a5`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previously reviewed baseline: `b961b4950febbc050081554bafe976b5d1b93f39`
- Reviewed upstream head: `63b29c2edd8ddcd63bad760dcae3b91cb2161d45`
- Live upstream head queried: `2c4bc2d7b2c5b7832ad964f39840d823d961cb0e`
- Query time: `2026-09-24T23:42:36+02:00`
- Requested range starts after `b961b4950febbc050081554bafe976b5d1b93f39`

This run reviews the first twenty commits of the range
`b961b495..2c4bc2d7`, ending at `63b29c2edd`. `A` is a complete safe
absorption or an already-equivalent local change. `B` needs a local
equivalent port and is deliberately not claimed as synchronized unless a
local commit is listed. `C` is rejected by ZZZ's local-first, no-account,
ACP-only boundary. The reviewed baseline advances to
`63b29c2edd8ddcd63bad760dcae3b91cb2161d45`.

## Decisions

| Upstream | Class | Local commit | Disposition                                                                           |
| -------- | ----- | ------------ | ------------------------------------------------------------------------------------- |
| 916fc2b8 | B     | 0e0a770a     | `hidden_files` uses `SplicingVec`; settings-UI path rename omitted.                   |
| 526c95d4 | C     | --           | Documentation link pass for removed account and native agent pages is not isolatable. |
| 0eda7703 | C     | --           | Accesskit adapter to drop is absent from ZZZ's `gpui_macos`.                          |
| 887149bb | B     | 0dd5047a     | Lenient glob parsing ported; absent `scan_symlinks`/`file_scan_depth` omitted.        |
| 8d085f19 | B     | 9407f17b     | VS Code `files.exclude` import ported; absent `file_scan_depth` omitted.              |
| 395def7a | C     | --           | Upstream merge-queue CI plumbing; workflow files are absent locally.                  |
| a51d23c9 | A     | 7d91b899     | Windows verbatim UNC path fix; cherry-picked with `-x -s`.                            |
| bcf6582c | B     | 9eed93e4     | Scoop `$SCOOP` discovery ported into `util::shell`.                                   |
| 85b8198e | A     | --           | Already equivalent: ZZZ pins `agent-client-protocol =2.2.0`, schema 1.9.1.            |
| df673c1b | C     | --           | `MessageContent` rewrite is entangled with unabsorbed `ContextCompaction`.            |
| af431aee | C     | --           | Requires the absent `editor::DiffStyleControls` widget.                               |
| 656e3aed | C     | --           | Requires absent display-terminal APIs from the rejected lifecycle work.               |
| 5a137cbc | B     | 927c2721     | `edit_predictions.disabled_globs` uses `SplicingVec`; `provider: zed` omitted.        |
| 51c03203 | A     | 942ef6f5     | Default keymaps close the active pane; cherry-picked with `-x -s`.                    |
| 4c7f3574 | B     | b5b16df8     | `edit_predictions_disabled_in` uses `SplicingVec`; settings UI keeps `lt(...)`.       |
| 418f8971 | B     | 46ff18b5     | Active repository inferred only from singleton items.                                 |
| 36726df0 | C     | --           | Threads-sidebar settings fields are absent from ZZZ.                                  |
| ca49b5da | C     | --           | Requires absent display-terminal APIs from the rejected lifecycle work.               |
| dc0bbb2b | A     | 5975d9fb     | Test-only completions speedup; cherry-picked with `-x -s`.                            |
| 63b29c2e | B     | 967054c1     | `terminal.path_hyperlink_regexes` uses `PathHyperlinkRegexes` with splice semantics.  |

Totals: four `A`, eight `B`, eight `C`.

## Applied work

Every direct `A` was created with `git cherry-pick -x -s`. Every `B` has a
local commit with full `Upstream:` / `Retained:` / `Omitted:` trailers. No
remote, pull request, or upstream remote was created.

- `9eed93e4` ports the Scoop path fix from `bcf6582c` into
  `crates/util/src/shell.rs`, where ZZZ keeps `find_pwsh_in_scoop`.
- `927c2721` switches `edit_predictions.disabled_globs` to `SplicingVec`,
  adds `find_value_range_in_json_text` to always-compiled `settings_json`
  (without upstream's `editing` feature gate), and imports Cursor's
  `globalCursorIgnoreList` into the inherited list.
- `b5b16df8` switches `edit_predictions_disabled_in` to `SplicingVec` and
  updates the localized settings-UI string in both catalogs.
- `46ff18b5` adds the singleton `ItemBufferKind` guard to
  `Workspace::set_active_item`; upstream's whitespace-only `ProjectDiff::new`
  hunk and its test module were dropped because ZZZ already contains them.
- `967054c1` adds `PathHyperlinkRegexes` and the `"..."` merge logic to the
  terminal settings, and ZZZ keeps its local `CursorShape` conversions ahead
  of the new test module.

## Per-commit narrative

### 916fc2b8 — B, `0e0a770a`

The `hidden_files` splice behavior is philosophy-safe and `SplicingVec`
already exists locally. Upstream also renames the Settings-UI `json_path`
from `worktree.hidden_files` to `hidden_files`; that belongs to a settings
page refactor ZZZ has not absorbed and would point at the wrong key, so it
was omitted. The upstream `build_worktree` test helper and its imports were
restored locally.

### 526c95d4 — C

Documentation-only navigation commit. Eleven of the 41 touched pages are
absent locally (`account/billing`, `plans-and-pricing`,
`business/organizations`, `agent-profiles`, `agents`, `instructions`,
`terminal-threads`, `use-api-access`, `use-a-gateway`, `use-a-local-model`,
`zed-agent`), and a dry-run cherry-pick conflicts across 20 existing pages
that have diverged. The retained subset would be a hand-filtered scattering
of 60-plus link edits, so the commit is not isolatable as a single change.

### 0eda7703 — C

The fix drops `MacWindowState::accesskit_adapter` before renderer teardown.
ZZZ's `gpui_macos` has no accesskit adapter or `SubclassingAdapter` wiring,
so there is no local equivalent to fix.

### 887149bb — B, `0dd5047a`

Ports `PathMatcher::new_lenient`, lenient `file_types` parsing, and the VS
Code `readonlyInclude` import. Upstream's `WorktreeSettings` also gains
`scan_symlinks` and `file_scan_depth`; ZZZ has neither field, so those
initializers and the irrelevant documentation were omitted. ZZZ test
helpers use `PathStyle::Posix` and `rel_path()` instead of upstream's
`PathStyle::Unix` and `RelPath::from_unix_str`.

### 8d085f19 — B, `9407f17b`

Maps VS Code `files.exclude` (not `files.watcherExclude`) into
`file_scan_exclusions` and shares `enabled_patterns` with
`files.readonlyInclude`. Upstream's `file_scan_depth: None` initializer was
omitted because the field does not exist locally.

### 395def7a — C

Upstream merge-queue CI plumbing. `.github/workflows/run_tests.yml` and
`tooling/xtask/src/tasks/workflows/run_tests.rs` are absent from ZZZ, and
CI/release automation is out of scope.

### a51d23c9 — A, `7d91b899`

Clean cherry-pick of the Windows `\\?\UNC\` path simplification. The
affected code and test are `#[cfg(target_os = "windows")]`, so verification
was a Windows cross-compile only; no Windows runtime test was run.

### 85b8198e — A, already equivalent

ZZZ's `Cargo.toml` already pins `agent-client-protocol = "=2.2.0"` and
`Cargo.lock` already has `agent-client-protocol-derive 2.2.0` and
`agent-client-protocol-schema 1.9.1` with the same checksums. No change.

### df673c1b — C

The `MessageContent` ordering rewrite touches `ContextCompaction`, which
ZZZ never absorbed; a dry-run cherry-pick produced 27 conflicts, including
the whole `ContextCompaction` surface being added from upstream context.
Rule 6 (unabsorbed prerequisite) applies.

### af431aee — C

Adds `FileDiffStyleToolbar` built on `editor::DiffStyleControls`. ZZZ has no
`DiffStyleControls`; its diff-style toggles are implemented inline in
`git_ui::solo_diff_view`. Porting would require either importing the
unabsorbed shared widget or duplicating the inline toggle logic.

### 656e3aed — C

Provider-owned terminal lifecycle depends on `Terminal::new_display`,
`is_process_backed`, and the read-only display-terminal machinery from
`df673c1b`/`656e3aed`; ZZZ lacks those APIs and `terminal_tool_header.rs`.

### 5a137cbc — B, `927c2721`

Ports the `disabled_globs` splice behavior, the settings-file range lookup,
and the Cursor/VS Code import changes. ZZZ's `settings_json` has no
`editing` feature, so `find_value_range_in_json_text` was made
unconditionally public and the `rc::Rc`/`DelayMs` imports ZZZ does not use
were dropped. Upstream's `"provider": "zed"` default was omitted, and the
reference docs keep ZZZ branding.

### 4c7f3574 — B, `b5b16df8`

Ports the `edit_predictions_disabled_in` splice behavior. The Settings-UI
description keeps ZZZ's `lt(...)` wrapper; both `en.json` and `zh-CN.json`
were updated to the new text.

### 418f8971 — B, `46ff18b5`

Retains only the `Workspace` guard that infers the active repository from
singleton items. ZZZ already contains the
`test_deploy_at_respects_active_repository_selection` assertion, so
upstream's test module and its whitespace-only `ProjectDiff::new` hunk were
dropped.

### 36726df0 — C

Groups `threads_sidebar_default_width` and `threads_sidebar_auto_open` into
`agent_settings::ThreadsSidebarSettings`. ZZZ has neither field, so the
refactor depends on unabsorbed settings work.

### ca49b5da — C

Makes provider-owned terminal views read-only. It depends on
`Terminal::is_process_backed` and related display-terminal APIs introduced
by the rejected `df673c1b`/`656e3aed` commits.

### dc0bbb2b — A, `5975d9fb`

Clean cherry-pick of a test-only change that replaces
`EditorLspTestContext::new_rust` with `EditorTestContext` and a fake LSP.

### 63b29c2e — B, `967054c1`

Ports `PathHyperlinkRegexes` and its splice/replace/clear merge behavior
plus the hyperlink-detection changes. ZZZ keeps its local `CursorShape`
conversions ahead of the new test module, and the docs and doc comments use
ZZZ branding.

## Verification

| Check                                                                                                | Result  |
| ---------------------------------------------------------------------------------------------------- | ------- |
| `git diff --check`                                                                                   | PASS    |
| `cargo fmt --all -- --check` (only pre-existing `agent_registry_store.rs`)                           | PASS    |
| `cargo check --locked -p settings_content -p worktree`                                               | PASS    |
| `cargo test --locked -p worktree --test integration hidden_files`                                    | PASS    |
| `cargo test --locked -p settings_content --lib hidden_files`                                         | PASS    |
| `cargo test --locked -p util --lib lenient`                                                          | PASS    |
| `cargo test --locked -p language --lib test_file_types_preserves_valid_patterns`                     | PASS    |
| `cargo test --locked -p worktree --test integration file_scan / invalid / inclusion / private_files` | PASS    |
| `cargo test --locked -p settings --lib import_file_exclusions`                                       | PASS    |
| `cargo test --locked -p settings --lib import_disabled_globs`                                        | PASS    |
| `cargo test --locked -p settings_json --lib find_value_range`                                        | PASS    |
| `cargo test --locked -p edit_prediction_ui --lib disabled_globs`                                     | PASS    |
| `cargo test --locked -p language --lib disabled_globs`                                               | PASS    |
| `cargo test --locked -p language --lib edit_predictions_disabled_in`                                 | PASS    |
| `cargo test --locked -p copilot --lib test_copilot_disabled_globs`                                   | PASS    |
| `cargo test --locked -p git_ui test_deploy_at_respects_active_repository_selection`                  | PASS    |
| `cargo test --locked -p editor --lib test_completion_with`                                           | PASS    |
| `cargo test --locked -p settings_content --lib path_hyperlink`                                       | PASS    |
| `cargo test --locked -p terminal --lib path_hyperlink`                                               | PASS    |
| `./script/check-keymaps`                                                                             | PASS    |
| `cargo check --locked --target x86_64-pc-windows-msvc -p util`                                       | PASS    |
| `cargo test --locked -p language --lib disabled_globs` (failed import, fixed)                        | PASS    |
| macOS / Windows runtime checks for platform hunks                                                    | NOT RUN |
| `cargo test --workspace`                                                                             | NOT RUN |

`cargo fmt --all -- --check` reports pre-existing drift in
`crates/project/src/agent_registry_store.rs`, unrelated to this run and not
changed.

## Run 2 — continuation after `63b29c2e`

### Scope

- Target branch: `sync/upstream-2026-09-24`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previously reviewed baseline: `63b29c2edd8ddcd63bad760dcae3b91cb2161d45`
- Reviewed upstream head: `e5221fa2956cd8ec88b459de0258a71fea5294de`
- Live upstream head queried: `7fdb97cad51694f2cdfd36abcb11265f4c4dafe2`
- Query time: `2026-09-25T02:35:07+02:00`
- Requested range starts after `63b29c2edd8ddcd63bad760dcae3b91cb2161d45`

This continuation reviews the next twenty commits of the range, from
`a49db374b6` through `e5221fa295`. The reviewed baseline advances to
`e5221fa2956cd8ec88b459de0258a71fea5294de`.

### Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                       |
| ---------- | ----- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| a49db374   | C     | --           | macOS-test backtrace workflow tuning; the CI workflows and generator are absent.                                                                  |
| b3f5de1fd1 | C     | --           | Nightly release cadence; release automation is out of scope and ZZZ's generator already diverged.                                                 |
| 4ab9b90b   | A     | --           | Already equivalent: the Baseten provider is absent from `LanguageModelProvider`.                                                                  |
| a434bb7e   | C     | --           | Atlas bind-group caching needs upstream's separate instance/texture bind groups; local combined layout makes it an unisolatable renderer rewrite. |
| bd61c4a2   | B     | bc37e7a5     | Nearby related diagnostics always link back to primary; test conflict resolved.                                                                   |
| fcee4a5a   | C     | --           | Upstream-only GPUI crates.io publishing helpers; ZZZ keeps its own `publish_gpui` tooling.                                                        |
| f25434f3   | A     | a13a7f95     | X11 full render after GPU recovery; cherry-picked with `-x -s`.                                                                                   |
| b54cc1d0   | A     | 81126825     | Block cursor text paints on arrival during animation; cherry-picked with `-x -s`.                                                                 |
| 2a415711   | B     | d1b54309     | `FakeFs` copy/overwrite and symlink-removal fixes; `RealFs::as_ref` test adaptation.                                                              |
| 05c2c9ab   | C     | --           | ACP terminal-streaming scaffolding; `write_output` diverged and the raw primitive has no production caller.                                       |
| 02238cd4   | B     | d1558c53     | Worktree roots skip the child-entry collision check when renaming.                                                                                |
| a4b9dc5f   | C     | --           | Mistral reasoning-effort APIs (`ReasoningEffort`, `supports_disabling_thinking`) are absent.                                                      |
| 62e5991d   | B     | 7b8b2706     | Bundled ConPTY bumped to 1.25.260710002 in `crates/zzz/build.rs` and the bundler.                                                                 |
| 189839ee   | B     | 19226687     | Deleted gutter markers stay visible at narrow custom widths.                                                                                      |
| 674dd790   | C     | --           | Pure ACP stdio/debug-log refactor into new `acp/` submodules; local `acp.rs` is consolidated, so the extraction is an unisolatable rewrite.       |
| d3600d66   | B     | 70c7282c     | ACP output drains before the exit error is reported, bounded to 250ms.                                                                            |
| 18ad8d11   | A     | --           | Already equivalent: `AddSelectionToThread` already accepts empty selections and reads the current line.                                           |
| a9829490   | B     | 4fafb31e     | Reversed row-highlight anchors no longer crash; `highlight_rows` test adapted to the local `Hsla` API.                                            |
| 16c9aa7e   | B     | ddc08697     | Grok 4.7 added to the manual xAI provider; the Supergrok half is omitted.                                                                         |
| e5221fa2   | C     | --           | Native `spawn_agent` subagent model selection; the native agent tools are absent.                                                                 |

Totals: four `A`, eight `B`, eight `C`.

### Applied work

Every clean `A` was created with `git cherry-pick -x -s`. Every `B` has a
local commit with a `sync:` subject and `Upstream:` / `Retained:` /
`Omitted:` trailers. No remote, pull request, or named upstream remote was
created.

- `bc37e7a5` keeps the upstream source change verbatim; the test conflict
  dropped upstream's unrelated `test_markup_content_diagnostic_messages_render_as_markdown`
  (absent locally) and kept `test_nearby_related_diagnostic_links_back_to_primary`.
- `d1b54309` keeps the upstream `FakeFs` source change; the new `RealFs`
  test passes `&fs` instead of `fs.as_ref()`, which `RealFs` does not
  implement locally.
- `d1558c53` ports the root-rename collision skip onto the local
  `Option<f32>` / `if let` shapes.
- `7b8b2706` updates the ConPTY nupkg URL in `crates/zzz/build.rs` and
  `script/bundle-windows.ps1`.
- `19226687` ports the deleted-marker width boost onto the local
  `Option<f32>` gutter setting.
- `4fafb31e` keeps the upstream `editor.rs`/`conflict_set.rs` source
  verbatim; the new regression test passes `Hsla` directly because local
  `Editor::highlight_rows` does not take a color closure.
- `ddc08697` adds `x_ai::Model::Grok47` and its reasoning efforts, omitting
  the `x_ai_subscribed` Supergrok half.
- `70c7282c` ports the ACP exit drain onto the local single-file `acp.rs`
  using the pinned `agent-client-protocol 2.2.0` `incoming_closed()` API.

### Per-commit narrative

#### `bd61c4a2` — B, `bc37e7a5`

The source hunk in `diagnostic_renderer.rs` removed the five-line distance
gate so nearby related diagnostics keep their back link. The expectation
updates in `test_diagnostics`, `test_diagnostics_with_folds`, and
`test_buffer_diagnostics` applied cleanly. The one conflict was a block
where upstream inserted its new test ahead of an unrelated diagnostics
test that ZZZ does not carry; resolution kept only the new test.

#### `2a415711` — B, `d1b54309`

Ports the `copy_file` overwrite/`ignore_if_exists` rewrite and the
symlink-aware `remove_file_inner`. `RealFs` in ZZZ does not implement
`AsRef<dyn Fs>`, so the new `test_realfs_copy_and_remove_semantics` passes
`&fs` (unsized coercion) rather than `fs.as_ref()`.

#### `02238cd4` — B, `d1558c53`

`populate_validation_error` and `rename_entry` now skip the child-entry
collision check when the entry is a worktree root, using the existing
`Project::entry_is_worktree_root`. Upstream's `match` rewrite and
`filename.to_owned()` were not imported; the local `if let` shapes are
kept.

#### `a4b9dc5f` — C

The Mistral provider in ZZZ has no `reasoning_effort` field, no
`mistral::ReasoningEffort`, and no `LanguageModel::supports_disabling_thinking`.
Only the `ZaiGlmLatest` enum variant could land, and it would be inert
without the unabsorbed reasoning-effort work, so the whole commit is
rejected.

#### `62e5991d` — B, `7b8b2706`

Bumps the bundled ConPTY to `1.25.260710002-preview` so the terminal grid
stays top-anchored when it grows on Windows. Upstream edits
`crates/zed/build.rs`; ZZZ keeps its `crates/zzz` path.

#### `189839ee` — B, `19226687`

`deleted_marker_base_width` now blends the custom width toward the line
height when the configured gutter is narrower than the default strip, so
the deleted pill keeps full line height. Ported onto the local
`Option<f32>` setting signature.

#### `674dd790` — C

The commit relocates ACP stdio construction, framing, and the debug log
into new `acp/transport.rs` and `acp/debug_log.rs` modules. ZZZ keeps all
of this consolidated in a single `acp.rs`, so the extraction is not
isolatable and changes no behavior on its own.

#### `d3600d66` — B, `70c7282c`

After the child exits, `exited_load_error_after_drain` waits (bounded by
`EXIT_DRAIN_TIMEOUT`, 250ms) for the SDK inbound stream to close and for a
`ForegroundBarrier` to acknowledge queued foreground work, concurrently
with stderr completion, before constructing `LoadError::Exited`.
`_stderr_task` becomes a `Shared<Task<()>>`. Upstream's two scripted-agent
regressions are omitted as test-only; the retained startup-failure test
now asserts the final stderr is present in the exit error.

#### `a9829490` — B, `4fafb31e`

Searches the row-highlight prefix up to `end_index` so an inverted
highlight range cannot produce an inverted slice, and emits empty conflict
sides as `anchor_before..anchor_after`. The parser change is in
`project/src/git_store/conflict_set.rs`. The new editor regression test
uses the local `Hsla`-taking `highlight_rows` signature.

#### `16c9aa7e` — B, `ddc08697`

Adds `x_ai::Model::Grok47` (`grok-4.7`), makes it the default xAI model,
and wires High reasoning effort for it. The `x_ai_subscribed` Supergrok
portion is omitted because that account-bound subscription crate is absent
from ZZZ.

#### `e5221fa2` — C

Adds an optional `model` parameter to the native `spawn_agent` tool and
native subagent model cards. ZZZ has no `spawn_agent_tool.rs` or
`list_agents_and_models_tool.rs`; the native agent tool surface is
absent.

### Verification

| Check                                                                                                              | Result  |
| ------------------------------------------------------------------------------------------------------------------ | ------- |
| `git diff --check`                                                                                                 | PASS    |
| `cargo check --locked -p agent_servers -p project_panel -p editor -p language_models -p x_ai -p fs -p diagnostics` | PASS    |
| `cargo check --locked -p gpui_linux`                                                                               | PASS    |
| `cargo test --locked -p agent_servers --lib startup_returns_error_when_agent_exits_before_initialization`          | PASS    |
| `cargo test --locked -p project_panel test_rename_root_of_worktree`                                                | PASS    |
| `cargo test --locked -p x_ai built_in_models_report_expected_metadata`                                             | PASS    |
| `cargo test --locked -p editor --lib test_deleted_marker_base_width`                                               | PASS    |
| `cargo test --locked -p editor --lib one_cell_movement_overlaps_target_only_after_advancing`                       | PASS    |
| `cargo test --locked -p editor --lib test_row_highlights_for_empty_conflict_side_after_edit`                       | PASS    |
| `cargo test --locked -p project --test integration test_parse_conflicts_with_empty_sides`                          | PASS    |
| `cargo test --locked -p fs --test integration copy_and_remove`                                                     | PASS    |
| `cargo test --locked -p diagnostics --lib test_nearby_related_diagnostic_links_back_to_primary`                    | PASS    |
| `cargo test --locked -p diagnostics --lib` (4 pre-existing failures)                                               | FAIL    |
| macOS / Windows runtime checks for platform hunks                                                                  | NOT RUN |
| `cargo test --workspace`                                                                                           | NOT RUN |

`cargo test --locked -p diagnostics --lib` fails the same four tests
(`test_diagnostics`, `test_buffer_diagnostics`,
`test_buffer_diagnostics_without_warnings`, `test_random_diagnostics_blocks`)
on the pre-run baseline `86b1a68cfd`; the failure is in
`crates/editor/src/test.rs:216` block-height bookkeeping and is unrelated
to this batch.

## Run 3 — continuation after `e5221fa295`

### Scope

- Target branch: `sync/upstream-2026-09-24`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previously reviewed baseline: `e5221fa2956cd8ec88b459de0258a71fea5294de`
- Reviewed upstream head: `b89471c52d22d2c642a9db6b4ad7e22f232bc974`
- Live upstream head queried: `a84858acb95381c8c40b730879ad0f9a0c70b294`
- Query time: `2026-09-25T12:21:09+02:00`
- Requested range starts after `e5221fa2956cd8ec88b459de0258a71fea5294de`

This continuation reviews the next twenty commits of the range, from
`96e95edac4` through `b89471c52d`. The reviewed baseline advances to
`b89471c52d22d2c642a9db6b4ad7e22f232bc974`.

### Decisions

| Upstream   | Class | Local commit | Disposition                                                                                         |
| ---------- | ----- | ------------ | --------------------------------------------------------------------------------------------------- |
| 96e95edac4 | B     | 6a162de286   | Atlas texture lookups return `Option`; a released texture skips the sprite batch.                   |
| ee8dd5f861 | C     | --           | ChatGPT subscription model picker; the account-bound provider is absent.                            |
| 75e9bb2fea | B     | fcce37d31e   | Claude Opus 5.5 BYOK model behavior; hosted half and compaction omitted.                            |
| bc6dd34d4c | B     | 1218fa972b   | GPT-6 Sol and Luna models; the compaction matrix is omitted.                                        |
| 5299a14401 | C     | --           | macOS Look Up needs the upstream markdown element refactor; local rendering has diverged.           |
| ba42ab9d98 | B     | 2f93addb94   | Standalone modifiers reach keystroke interceptors; the test module is omitted.                      |
| 7fecbb2c4b | C     | --           | which-key and pending-indicator unification rewrites a module ZZZ has trimmed.                      |
| b7be0f185b | B     | 1706daa522   | REPL output copy uses `CopyButton`; the localized tooltip is kept.                                  |
| f2aa97c47f | B     | 90f9a798f8   | Thread registration seeds permissions already waiting for confirmation.                             |
| a3ccafef66 | C     | --           | Unisolatable ACP session-notice feature depends on absent idle-sleep and compaction surfaces.       |
| 05958c4773 | B     | bcfd168880   | Elicitation completion is emitted from state transitions.                                           |
| 6971ae21e9 | C     | --           | opencode dynamic model catalog adds a default network surface and an unisolatable provider rewrite. |
| bf89346c1b | C     | --           | Community PR routing metadata.                                                                      |
| a39324bf99 | A     | 4ce8be2e60   | Sidebar keyboard-selection border; cherry-picked with `-x -s`.                                      |
| fbb313eb33 | C     | --           | CI workflow Danger runner profile.                                                                  |
| f57e400a17 | C     | --           | Zed version bump; release metadata.                                                                 |
| 573b5d66bb | A     | fc1d2ffa0a   | Helix buffer picker opens in the active pane; cherry-picked with `-x -s`.                           |
| 9183ba4c55 | B     | 67603e8092   | Confirm with no selection dismisses; a11y gating is already absent locally.                         |
| eb2f43d982 | C     | --           | `*Map` randomized-test fix depends on an unabsorbed `path_key` excerpt-reuse rewrite.               |
| b89471c52d | C     | --           | Upstream-only `gpui_apple` dependency cleanup and xtask workspace plumbing; macOS-only tooling.     |

Totals: two `A`, eight `B`, ten `C`.

### Applied work

Every clean `A` was created with `git cherry-pick -x -s`. Every `B` has a
local commit with a `sync:` subject and `Upstream:` / `Retained:` /
`Omitted:` trailers. No remote, pull request, or named upstream remote was
created.

- `6a162de286` ports the atlas safety net to `gpui_wgpu`, `gpui_macos`,
  and `gpui_windows`: lookups return `Option` and each sprite draw skips a
  batch whose texture was released. ZZZ's wgpu draw helpers keep their
  `bool` overflow return and report a skipped batch as success; the cached
  wgpu bind-group path does not exist locally and was not imported.
- `fcce37d31e` teaches the Anthropic provider that `claude-opus-5-5`
  enables fast mode, prefix-bound thinking blocks, forced-tool-use
  exclusion, and always-on adaptive thinking with temperature omitted.
  The hosted `language_models_cloud` half is absent from ZZZ and the
  `supports_compaction` model matrix does not exist locally.
- `1218fa972b` adds `x_ai::Model`'s sibling `open_ai::Model::SixSol` and
  `SixLuna` with their limits, reasoning efforts, and effort-dependent
  temperature rejection. The `supports_compaction` matrix was omitted for
  the same reason. The regression test was adapted to ZZZ's
  `into_open_ai_response` signature.
- `2f93addb94` passes the recognized keystroke, including standalone
  modifiers observed on release, to keystroke interceptors and observers
  in `gpui::Window`; the keymap editor records and searches modifier-only
  strokes. The `key_dispatch` test additions are test-only and depend on
  pending-input helpers ZZZ does not carry.
- `1706daa522` switches the REPL output copy control to the shared
  `CopyButton` and keeps the localized `repl.outputs.copy_output` tooltip.
- `90f9a798f8` seeds a conversation's pending permission requests from
  tool calls already `WaitingForConfirmation` when a thread is registered,
  sharing the deduplicating helper with later request events. The upstream
  test is test-only and depends on absent `pending_tool_call_for_session`
  and `AuthorizationKind` APIs.
- `bcfd168880` routes elicitation completion through the store's response
  and cancel state transitions, resolving waiters and emitting change
  events consistently. The upstream test module is omitted as test-only.
- `67603e8092` makes `ContextMenu::confirm` emit `DismissEvent` when no
  item is selected. The first-entry-on-open behavior the upstream commit
  fixes is already absent locally, and `Window::is_a11y_enabled` would
  need the AccessKit writer path ZZZ does not carry.

### Per-commit narrative

#### `96e95edac4` — B, `6a162de286`

Each rendering backend looked up a sprite batch's atlas texture with an
`unwrap`, so a stale paint that referenced a released texture aborted the
process. ZZZ ships wgpu on Linux, Metal under `gpui_macos`, and DirectX
under `gpui_windows`, so all three backends were ported. `gpui_wgpu`'s
`get_texture_info` now returns `Option` through the existing
`WgpuAtlasStorage::get`, and its three `draw_*` helpers return `true`
(skip, no overflow) on a missing texture. The unused
`Index<AtlasTextureId>` impl was removed. `gpui_macos` and `gpui_windows`
got the same `Option` treatment and early return at their call sites.

#### `ee8dd5f861` — C

Raises the client version sent to the ChatGPT subscription catalog so newer
account models appear. ZZZ has no `openai_subscribed` provider and no
account or subscription surface, so there is nothing to update.

#### `75e9bb2fea` — B, `fcce37d31e`

`claude-opus-5-5` joins the fast-mode, prefix-binding, and forced-tool-use
model-ID predicates, and `completion.rs` forces `AdaptiveThinking` with
`temperature: None` for it. ZZZ's Anthropic lineage does not include
`claude-opus-5`, so only `claude-opus-5-5` was added ahead of the existing
`4-6`/`4-7`/`4-8` entries. The `language_models_cloud` diff is absent
locally, and `Model` has no `supports_compaction` field to extend.

#### `bc6dd34d4c` — B, `1218fa972b`

Adds `gpt-6-sol` and `gpt-6-luna` as selectable models with 1,050,000-token
contexts, 128,000 output tokens, medium default reasoning, the full
`None..Max` effort range, and effort-dependent temperature rejection
(Sol/Luna reject temperature unless reasoning is explicitly disabled).
ZZZ's `display_name` returns IDs rather than upstream's title-cased names,
so the new arms follow the local style. The `supports_compaction` matrix
and its documentation comment were dropped because the method has no local
counterpart or caller.

#### `5299a14401` — C

Registers a macOS Look Up input handler for rendered Markdown and rewrites
the line rendering into `RenderedLineElement` with clipped visible bounds
and UTF-16 source mapping. ZZZ's `markdown.rs` has diverged: it has no
`HighlightedLine` split, a different `RenderedText` shape, and a
`bounds_for_source_range` already used for other purposes. The lookup
support cannot be isolated from that refactor.

#### `ba42ab9d98` — B, `2f93addb94`

`dispatch_keystroke_interceptors` and `dispatch_keystroke_observers` now
take a recognized `Keystroke` and an `Option<&dyn Action>` instead of
downcasting a raw event, so standalone modifiers recognized on release
reach interceptors before keymap dispatch. The keymap editor and its
keystroke input recorded the modifier. All `key_dispatch.rs` changes are
inside `mod tests` and depend on pending-input test helpers absent
locally, so that file was left at its ZZZ state.

#### `7fecbb2c4b` — C

Unifies which-key preparation and rendering with the pending-keystrokes
indicator behind a new `pending_bindings.rs` and shrinks `which_key_modal`.
ZZZ's `which_key` crate is a trimmed implementation with no pending
indicator and a different modal shape, so the refactor is not isolatable.

#### `b7be0f185b` — B, `1706daa522`

The copy control on REPL output becomes a `CopyButton`, giving the green
checkmark and popup feedback. ZZZ localizes the tooltip, so
`tooltip_label` receives `tr(cx, "repl.outputs.copy_output", "Copy Output")`
instead of upstream's literal. The shared button is no longer styled
`ButtonStyle::Transparent`, matching upstream.

#### `f2aa97c47f` — B, `90f9a798f8`

`Conversation::register_thread` now reads the thread's entries before
subscribing and records any `WaitingForConfirmation` tool calls, using a
new `add_permission_request` that deduplicates. The upstream regression
test needs `pending_tool_call_for_session` and
`acp_thread::AuthorizationKind`, neither present locally, so it was not
carried.

#### `a3ccafef66` — C

Adds ACP session notices end to end: an `acp::Notice` store and
`NoticesUpdated` event on `AcpThread`, client session capabilities, a
session-notice callout, and thread-view wiring. The upstream context also
depends on idle-sleep prevention and `AgentThreadEntry::ContextCompaction`
machinery ZZZ never absorbed, and the feature touches eight files with
conflicts across the absent surfaces, so it is rejected rather than
partially imported.

#### `05958c4773` — B, `bcfd168880`

`ElicitationStore` now emits a completion change when a request is
responded to, cancelled, or has its waiter dropped, and
`AcpThread::emit_elicitation_change` funnels those transitions. The two
conflict regions were both inside `mod tests`; the production hunks applied
cleanly, so the local test module was kept wholesale.

#### `6971ae21e9` — C

Replaces the static opencode model enum with a runtime fetch of
`https://models.opencode.ai/api.json` and rewrites the provider's model
surface (~1,600 changed lines) plus a docs page ZZZ does not carry.
Fetching a third-party catalog on provider use adds a default network
surface, and the provider rewrite is not isolatable from the absent
dynamic-model architecture.

#### `bf89346c1b` — C

One-line change to `script/community-pr-track-mapping.json`, a
community/upstream automation artifact with no local counterpart.

#### `a39324bf99` — A, `4ce8be2e60`

Clean cherry-pick of the Workspace sidebar selection border that matches
the project panel focus style.

#### `fbb313eb33` — C

CI workflow and xtask plumbing to run Danger on its dedicated runner.
`.github` workflows and the generator are out of scope locally.

#### `f57e400a17` — C

Bumps the upstream Zed crate to `v1.23.0` in `Cargo.lock` and
`crates/zed/Cargo.toml`. Release metadata does not apply to ZZZ.

#### `573b5d66bb` — A, `fc1d2ffa0a`

Clean cherry-pick rebinding Helix `space b` to
`tab_switcher::OpenInActivePane`, which already exists locally.

#### `9183ba4c55` — B, `67603e8092`

Upstream gates the "select the first item when a menu opens" behavior on
`Window::is_a11y_enabled`, which ZZZ cannot provide because the AccessKit
writer path is absent. ZZZ's `ContextMenu` already has no
`suppress_focus_selection` or focus-in selection logic, so the flicker bug
does not exist here. The retained change is the new dismiss-on-confirm
behavior when nothing is selected.

#### `eb2f43d982` — C

Fixes randomized editor map tests across `buffer_diff`, `block_map`,
`fold_map`, `wrap_map`, `multi_buffer`, and `path_key`. The `path_key`
hunks extend the `reused_excerpt`/`context_anchors_match` excerpt-reuse
logic, which ZZZ's divergent `path_key.rs` does not have, so the commit
depends on an unabsorbed prerequisite.

#### `b89471c52d` — C

Drops `block` as a direct `gpui_apple` dependency and adds workspace
classification support to xtask. ZZZ has no `gpui_apple` crate and the
change is macOS-only tooling.

### Verification

| Check                                                                                               | Result  |
| --------------------------------------------------------------------------------------------------- | ------- |
| `git diff --check`                                                                                  | PASS    |
| `cargo fmt --all -- --check` (only pre-existing `agent_registry_store.rs` drift)                    | PASS    |
| `cargo check --locked -p gpui_wgpu`                                                                 | PASS    |
| `cargo check --locked -p anthropic`                                                                 | PASS    |
| `cargo test --locked -p anthropic --lib fast_mode`                                                  | PASS    |
| `cargo check --locked -p open_ai`                                                                   | PASS    |
| `cargo test --locked -p open_ai --lib request_conversion_omits_unsupported_temperature`             | PASS    |
| `cargo check --locked -p gpui`                                                                      | PASS    |
| `cargo check --locked -p keymap_editor`                                                             | PASS    |
| `cargo check --locked -p repl`                                                                      | PASS    |
| `cargo check --locked -p agent_ui`                                                                  | PASS    |
| `cargo check --locked -p acp_thread`                                                                | PASS    |
| `cargo check --locked -p ui`                                                                        | PASS    |
| `cargo test --locked -p acp_thread --lib` (78 passed, 1 baseline failure)                           | FAIL    |
| `cargo check --locked --target aarch64-apple-darwin -p gpui_macos` (atlas/keystroke platform hunks) | NOT RUN |
| `cargo check --locked --target x86_64-pc-windows-msvc -p gpui_windows` (atlas backend)              | NOT RUN |
| macOS / Windows runtime checks for platform hunks                                                   | NOT RUN |
| `cargo test --workspace`                                                                            | NOT RUN |

`cargo test --locked -p acp_thread --lib` fails
`test_terminal_kill_allows_wait_for_exit_to_complete` with "Timed out
waiting for printf output to appear in terminal". The same test fails on
the unmodified `acp_thread.rs` baseline, so it is a pre-existing
environment failure unrelated to this batch. `cargo fmt --all -- --check`
reports the same pre-existing `crates/project/src/agent_registry_store.rs`
drift recorded in Run 1.
