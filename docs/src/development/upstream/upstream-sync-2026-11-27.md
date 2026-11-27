---
title: Upstream Sync 2026-11-27
description: Selective Zed upstream sync, twenty commits after 831229fe70.
---

# Upstream Sync 2026-11-27

## Scope

- Local base: `sync/upstream-2026-11-27` from `main` at `77e5f9811c`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `831229fe70bbb126d49cde0b703ae6a8b25893d8`
- Reviewed range: `831229fe70..aa06397a1d`, 20 commits, oldest first
- New reviewed baseline: `aa06397a1d88189b779b3c781c3e6835aa0cb888`
- Live head at query time: `46375fbf64574f01dc08fd2d2998aa7bbb7f0377`
- Remaining after this batch: 36

Batch H is the first window where the sandbox work reaches full volume:
three of twenty commits rewrite `crates/sandbox`, which this tree does
not carry, a fourth adds the Windows ARM CI job that runs its tests, and
a fifth bumps a lint pin in workflows that do not exist here. The batch
also contains the student-plan copy and the account documentation, both
of which describe a billing surface ZZZ removed. What is left is
ordinary product work, and six commits of it port: the deferred-content
mask, the multibuffer close bug, the Git panel focus and padding fixes,
the excerpt-boundary walk, and the pinned-tab restore. Two commits landed
as clean cherry-picks.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                             |
| ---------- | ----- | ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 990491f0ea | B     | 66c67ba06e   | Reads the GPU driver version from one DXGI call instead of vendor shims. Payload below.                                                                                 |
| c506354e4c | C     | --           | Steers the Windows AppContainer sandbox away from packaged PowerShell; `crates/sandbox` and `crates/agent/src/sandboxing.rs` are absent.                                |
| 168964ea32 | C     | --           | Thread-only model, profile and effort picks; needs `crates/agent/src/thread.rs`, `language_model/src/registry.rs` and a `conversation_view.rs` this tree diverged from. |
| 6837f17378 | B     | c9230e33c9   | Applies deferred content masks during prepaint. Payload below.                                                                                                          |
| d990d327bc | A     | f4fb3af467   | Clean cherry-pick; wrapped Git panel headers keep their vertical padding.                                                                                               |
| 79af38b26c | C     | --           | Keeps measured layouts while their inputs are unchanged; needs `90549abcfd`, which is already rejected, and a `randomized_element_tree.rs` this tree never had.         |
| 90c27c388c | A     | 26ae18e80b   | Clean cherry-pick; the Java page asks for JDK 25.                                                                                                                       |
| b67017634d | C     | --           | Student plan model-access docs; `docs/src/account/` does not exist here.                                                                                                |
| 17fe8c6117 | C     | --           | Bumps cargo-shear to 1.14.1 in workflows and xtask files this tree does not have; local linting is `cargo-machete`.                                                     |
| 2b248e6072 | C     | --           | Student plan copy; needs `crates/language_models`, the cloud provider and `LanguageModelRegistry`, none of which exist here.                                            |
| 36acbc678f | C     | --           | Closes macOS seatbelt protected-path bypasses in `crates/sandbox/src/macos_seatbelt.rs`, which does not exist here.                                                     |
| 6673bb0138 | C     | --           | Runs sandbox tests on `windows-11-arm`; `.github/workflows/run_tests.yml` and its xtask twin are absent, and the crate is absent.                                       |
| 3b6227877b | C     | --           | Shows closed checkouts in the worktree picker; `crates/git_ui_core` and `crates/collab` are absent, and the pick conflicted in ten paths.                               |
| 05c93905c5 | B     | 3ff976595f   | Closes multibuffers without closing their files. Payload below.                                                                                                         |
| 67c513c8a1 | B     | 88d78fa40c   | Focuses the Git panel when there is no repository. Payload below.                                                                                                       |
| e7d5336158 | C     | --           | Keeps system network services out of the proxy sandbox; the whole change is in `crates/sandbox`, with a one-line caller in `acp_thread`.                                |
| f02fe65230 | C     | --           | Ollama thinking-level selection; `crates/ollama` and `crates/language_models` are absent.                                                                               |
| b0e921dadc | B     | e29986207b   | Stops the excerpt boundary walk at the range end. Payload below.                                                                                                        |
| 2acb26ac1a | C     | --           | Frame and input latency reporting diverged; local `profiler.rs` is 744 lines against a 2226-line pre-image and the pick conflicted in eight files.                      |
| aa06397a1d | B     | 2947386030   | Keeps pinned tabs when restoring past `max_tabs`. Payload below.                                                                                                        |

Totals: two `A` (both landed), six `B`, twelve `C`.

## Applied Work

- `d990d327bc` and `90c27c388c` used `git cherry-pick -x -s` against a
  clean tree and kept the upstream messages.
- `990491f0ea`, `6837f17378`, `05c93905c5`, `67c513c8a1`, `b0e921dadc`
  and `aa06397a1d` started as cherry-picks, conflicted, and were committed
  with the port payload below.
- `c506354e4c`, `168964ea32`, `79af38b26c`, `b67017634d`, `17fe8c6117`,
  `2b248e6072`, `36acbc678f`, `6673bb0138`, `3b6227877b`, `e7d5336158`,
  `f02fe65230` and `2acb26ac1a` changed no product code. Each was checked
  with a dry-run cherry-pick before it was rejected; the conflicting paths
  and hunk counts below come from those runs.

No named upstream remote, push, or pull request was created.

## Per-Commit Narrative

### 990491f0ea — GPU driver version without vendor shims (B)

Retained: the driver version now comes from one DXGI
`CheckInterfaceSupport(&IDXGIDevice::IID)` call. The `nvapi64.dll` and
`amd_ags_x64.dll` loading, the `nvidia`, `amd` and `dxgi` shim modules,
and the `with_dll_library` helper are gone, so a packaged build no longer
ships `amd_ags_x64.dll` and the bundler no longer downloads the AGS
runtime (`AGS_URL`, `AGS_SHA256`, `AGS_DIRECTORY`, `ags_runtime()`).

Adapted: the crate reaches helper traits as `::util::ResultExt` here, and
the removed code left `anyhow::Context` unused in `util.rs`. The
`typos.toml` entries for `ags`/`AGS`/`Ags` were dropped along with the
code they exempted. `GpuSpecs` is `#[non_exhaustive]` here, so the port
keeps the local `GpuSpecs::default()` plus field assignments instead of
upstream's struct literal; `cargo check --target x86_64-pc-windows-msvc`
is what caught it, since a Linux build compiles none of this crate.

The AGS removal also reaches one file upstream does not have:
`tooling/windows_msi_builder/Program.cs`, the local-only MSI builder,
downloaded the same SDK and staged `amd_ags_x64.dll` into the installer.
With no consumer left, its `AgsUrl`/`AgsArchiveName` constants,
`StageAmdGpuServicesAsync` and its x86_64-gated call site are gone; the
`ags`/`AGS`/`Ags` spellings dropped from `typos.toml` are what surfaced
it, because the builder still matched every one of them.

Omitted: nothing behavior-bearing. `script/bundle-windows.ps1` does not
exist in this tree — it went away with `20e7c388ff` — so that hunk had no
host; the local Windows installer is
`crates/zzz/resources/windows/zzz.iss`.

### c506354e4c — packaged PowerShell in the Windows sandbox (C)

Two of the three files are `crates/sandbox/src/sandbox.rs` and
`crates/sandbox/src/windows_psec.rs`, and the third is
`crates/agent/src/sandboxing.rs`. This tree carries none of them: the
native-agent sandbox is a rejected surface, so there is no AppContainer
job object to steer away from packaged PowerShell.

### 168964ea32 — thread-only picks (C)

The commit adds a `thread_only` flag and a `picker` dependency, and
threads them through the agent's `Thread`, the model, profile and effort
selectors, the thread metadata store and the settings content. Local
`crates/agent` has no `Thread` type at all — `agent.rs` is 28 lines of
re-exports over `db`, `thread_store`, `tools` and permissions, against a
10543-line upstream pre-image — and `crates/agent/src/thread.rs`,
`crates/language_model/src/registry.rs` and `docs/src/ai/agent-profiles.md`
do not exist. `conversation_view.rs` is 9357 lines against a 21912-line
pre-image and `thread_view.rs` is 10380 against 16186.

The dry-run agreed with that reading: 28 conflict hunks across
`agent.rs`, `db.rs`, `agent_panel.rs`, `agent_ui.rs`, `conversation_view.rs`,
`thread_view.rs`, `model_selector.rs`, `profile_selector.rs`,
`settings_content/src/agent.rs` and two docs pages, plus three
modify/delete conflicts for the files this tree never had. Landing it
means importing the upstream thread model, not porting this commit.

### 6837f17378 — deferred content masks during prepaint (B)

Retained: deferred prepaint runs inside `Window::with_content_mask`, so a
masked deferred element's hitboxes stop at its visible area instead of
blocking clicks on the elements underneath.
`test_deferred_content_mask_clips_hitboxes` covers the clipped and
unclipped halves.

Omitted: the upstream `crates/gpui/src/window.rs` hunk. Local moved the
deferred prepaint loop to `window/methods.rs`, and only that copy runs.

### d990d327bc — vertical padding in the wrapped panel header (A)

A one-line layout change to `crates/git_ui/src/git_panel.rs`; the pick was
clean.

### 79af38b26c — measured layouts across frames (C)

The commit itself opens by depending on the layout work that came before
it: "With layout nodes kept between frames…". That prerequisite is
`90549abcfd`, which batch G rejected and which sits in the ledger at
`upstream-rejected.tsv:528`. On top of that, local `taffy.rs` is 903 lines
against a 1303-line pre-image, with no `LayoutCache`, `SecondaryMap` or
`request_measured_layout`, and the commit's test file,
`crates/gpui/src/randomized_element_tree.rs`, has never existed in this
tree.

The dry-run conflicted in twelve hunks across `taffy.rs` (8), `text.rs`
(2), `line_layout.rs` (1) and `window.rs` (1), and wanted to recreate
`randomized_element_tree.rs`. A commit that needs a rejected prerequisite
is rejected by the same rule that rejected the prerequisite.

### 90c27c388c — Java 25 requirements (A)

Docs-only; the pick was clean.

### b67017634d — student plan model access (C)

Both files are `docs/src/account/plans-and-pricing.md` and
`docs/src/account/zed-hosted-models.md`. `docs/src/account/` does not
exist here; hosted-model plans are part of the removed commercial
surface, so there is no page to correct.

### 17fe8c6117 — cargo-shear 1.14.1 (C)

The pin is duplicated into `.github/workflows/run_tests.yml`,
`.github/workflows/autofix_pr.yml`, their two xtask generators and
`crates/agent_ui_benchmarks/Cargo.toml`. Neither workflow, neither
generator nor that crate exists here — `.github/workflows/` holds only
`ci.yaml` and `pr-metadata.yml` — and the tree has no cargo-shear
invocation to version: local unused-dependency checking is
`cargo-machete`, run from `cargo xtask clippy`.

### 2b248e6072 — student plan copy (C)

The behavior change is a saved-model check in the commit-message
configuration tooltip, which reads `LanguageModelRegistry` and the cloud
provider's model list. Local has neither: `crates/language_models` is
absent, `crates/language_model` holds only `api_key.rs`,
`language_model.rs` and `request.rs`, and there is no
`LanguageModelRegistry`. The same commit also edits
`crates/ai_onboarding/src/plan_definitions.rs`, and `crates/ai_onboarding`
does not exist. The remaining prose changes describe plans ZZZ does not
sell.

### 36acbc678f — macOS protected-path bypasses (C)

388 inserted lines, all in `crates/sandbox/src/macos_seatbelt.rs`. The
crate is absent.

### 6673bb0138 — sandbox tests on Windows 11 ARM (C)

Adds a job to `.github/workflows/run_tests.yml`, its xtask generator
`tooling/xtask/src/tasks/workflows/run_tests.rs`, and a
`WINDOWS_11_ARM` runner constant. The workflow, the generator and the
`crates/sandbox` tests the job runs are all absent; local CI is the
hand-written `ci.yaml`.

### 3b6227877b — closed checkouts in the worktree picker (C)

2364 of the 3144 inserted lines are in
`crates/git_ui_core/src/worktree_picker.rs`, and this tree has no
`crates/git_ui_core` — the picker lives at
`crates/git_ui/src/worktree_picker.rs`, 2266 lines against a 3628-line
pre-image and a 5769-line post-image. The commit also edits
`crates/collab`, and
`crates/proto/proto/git.proto`, `crates/project/src/git_store.rs`,
`crates/fs/src/fs.rs` and `crates/git/src/repository.rs` grow new scan
results that the picker consumes.

The dry-run conflicted in ten paths, two of them modify/delete for
`crates/collab/src/db/queries/{projects,rooms}.rs`. Reconstructing the
feature means re-deriving a 2364-line UI against a diverged picker and
then designing the proto and git-store halves without `collab`, which is
not an isolatable port.

### 05c93905c5 — close multibuffers without closing their files (B)

Retained: `ItemHandle::single_file_project_path` returns a path only for
singleton items and `InvalidItemView`, and `:bd`,
`close_item_in_all_panes` and the tab switcher match items across panes
through it, so a multibuffer is closed on its own instead of as the file
under its cursor. `test_close_by_path_keeps_multibuffers_and_files_apart`
covers it.

Omitted: the upstream `open_in_active_pane` boolean, which local
`TabSwitcherDelegate` replaced with the
`mode == TabSwitcherMode::OpenInActivePane` check; only the field
reference was reshaped, the condition is unchanged. One upstream test,
`test_close_item_in_all_panes_closes_invalid_items`, was dropped because
it asserts on pane and item APIs this tree does not expose, and its
behavior is already covered by the retained test.

### 67c513c8a1 — focus the Git panel with no repository (B)

Retained: `GitPanel::focus_editor` returns early and
`activation_focus_handle` refuses the commit editor when
`active_repository` is `None`, so `git_panel::ToggleFocus` and `Toggle`
leave the panel focused and a second `Toggle` closes the dock.
`test_toggle_without_repository` covers it.

Omitted: the upstream `Panel::activation_focus_handle` hunk, because
local `GitPanel` already delegates that method to its inherent one.

### e7d5336158 — system network services in the proxy sandbox (C)

The change adds `allow_system_network_services` to `SandboxPolicy` and
threads it through `crates/sandbox/src/{sandbox,macos_seatbelt}.rs` and
`crates/agent/src/sandboxing.rs`. Only the one-line caller in
`crates/acp_thread/src/terminal.rs` has a local host, and it references
`sandbox::SandboxPolicy`, which this tree does not compile.

### f02fe65230 — Ollama thinking levels (C)

307 of the 389 inserted lines are in
`crates/language_models/src/provider/ollama.rs` and the rest in
`crates/ollama/src/ollama.rs`. Both crates are absent here, and
`crates/language_models` has no local replacement for a provider
registry.

### b0e921dadc — excerpt boundary walk stops at the range end (B)

Retained: `excerpt_boundaries_in_range` seeks to the start of the current
excerpt before checking whether the cursor sits on a diff transform, so a
range starting mid-excerpt no longer begins the walk one excerpt early.
The walk returns at the first excerpt past the range instead of skipping
it one excerpt at a time, which keeps a one-line edit in file k from
costing a pass over every excerpt after k. `check_multibuffer` checks
random ranges with all three bound forms,
`test_excerpt_boundaries_in_subranges_with_diff_hunks` pins the walk over
expanded diff hunks, and
`test_excerpt_boundaries_in_range_stops_at_range_end` counts visited
excerpts through the test-only `EXCERPT_BOUNDARY_STEPS` counter.

Local divergence: `next_range.start.value.expect("value should be present")`
keeps the local expect style where upstream uses `unwrap()`.

### 2acb26ac1a — frame and input latency across platforms (C)

The commit reworks the `FrameEvent` model itself: upstream replaces
`Invalidated` and `DrawStarted` with `Draw(FrameTiming)` and
`Present(PresentTiming)`, adds `record_frame_throttled`, the 1 ms and
2 ms buckets in `FRAME_DURATION_BUCKETS_MS`, and stamps frame signals on
Wayland `Expose`, X11 and Windows. Local `profiler.rs` is 744 lines
against a 2226-line pre-image, has no `WindowProfiler`, no
`PresentTiming` and no buckets, and `crates/gpui_linux` has no
`scheduled_frame_at` to remove. `input_latency_ui` reads local
`InputLatencySnapshot` and has no telemetry dependency.

The dry-run conflicted in seventeen hunks across eight files
(`profiler.rs` 4, Wayland window 3, `gpui_windows/src/platform.rs` 3,
`window.rs` 2, `events.rs` 2, `x11/client.rs` 1, `gpui_windows/src/window.rs`
1, `input_latency_ui.rs` 1), including the enum rewrite above. The
divergence is architectural, not layout.

### aa06397a1d — pinned tabs past `max_tabs` (B)

Retained: `SerializedPane::deserialize` collects the item handles, sets
the pinned count to the number of pinned items that actually restored,
and only then adds the items, so `max_tabs` can no longer close the
restored pinned tabs. `test_restoring_pinned_tabs_past_max_tabs` covers
both the all-restored and the failed-pinned cases.

Omitted: the upstream `crates/sandbox/src/macos_seatbelt.rs` hunk, a
test-only sandbox fixture in a crate ZZZ does not carry.

## Verification

| Check                                                                                                | Result  | Notes                                                                                                                                                                                                                                        |
| ---------------------------------------------------------------------------------------------------- | ------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `git diff --check` over the batch                                                                    | PASS    |                                                                                                                                                                                                                                              |
| `cargo fmt --all -- --check`                                                                         | PASS    |                                                                                                                                                                                                                                              |
| `./script/check-dco`                                                                                 | PASS    | 50 non-merge commits signed off.                                                                                                                                                                                                             |
| `cargo check --locked -p git_ui -p multi_buffer -p workspace -p tab_switcher -p xtask --all-targets` | PASS    | After all eight local commits.                                                                                                                                                                                                               |
| `cargo test --locked -p gpui test_deferred_content_mask_clips_hitboxes`                              | PASS    | 1 passed, from the `6837f17378` port.                                                                                                                                                                                                        |
| `cargo test --locked -p workspace test_restoring_pinned_tabs_past_max_tabs`                          | PASS    | 1 passed, from the `aa06397a1d` port.                                                                                                                                                                                                        |
| `cargo test --locked -p git_ui test_toggle_without_repository`                                       | PASS    | 1 passed, from the `67c513c8a1` port.                                                                                                                                                                                                        |
| `cargo test --locked -p tab_switcher`                                                                | PASS    | 14 passed, including `test_close_by_path_keeps_multibuffers_and_files_apart`.                                                                                                                                                                |
| `cargo test --locked -p multi_buffer --lib`                                                          | PASS    | 61 passed, including the three excerpt-boundary tests.                                                                                                                                                                                       |
| `./script/backfill-upstream-ledger` and `cargo xtask check upstream-ledger`                          | PASS    | Ledger regenerated to 924 rejection rows from this report's decision table.                                                                                                                                                                  |
| Prettier over this report                                                                            | PASS    | `3.9.10`, the version pinned by `script/prettier`.                                                                                                                                                                                           |
| `cargo check --locked -p gpui_windows --target x86_64-pc-windows-msvc`                               | PASS    | The Windows std is installed on this host, so the crate's code is type-checked. Caught `GpuSpecs` being `#[non_exhaustive]` locally; fixed in the `990491f0ea` port.                                                                         |
| `cargo xtask check all`                                                                              | PASS    | All 25 repository checks.                                                                                                                                                                                                                    |
| `cargo xtask clippy`                                                                                 | PASS    | Workspace, manifest and `future_not_send` passes, then `cargo-machete` and `typos`. First run failed in `typos` on the leftover AGS staging in `tooling/windows_msi_builder`; `buf` is not installed on this host, so that stage is skipped. |
| macOS, Windows, iOS and sandbox runtime behavior                                                     | NOT RUN | This host is Linux. Nothing was executed on those platforms.                                                                                                                                                                                 |
