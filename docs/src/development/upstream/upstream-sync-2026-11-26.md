---
title: Upstream Sync 2026-11-26
description: Selective Zed upstream sync, twenty commits after f00a94318c.
---

# Upstream Sync 2026-11-26

## Scope

- Local base: `sync/upstream-2026-11-26` from `main` at `b3a1def633`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `f00a94318c059f03892e04f82343e899fbfaa844`
- Reviewed range: `f00a94318c..831229fe70`, 20 commits, oldest first
- New reviewed baseline: `831229fe70bbb126d49cde0b703ae6a8b25893d8`
- Live head at query time: `e4fa16422543ab77b129d7f4ad825630b993ecdd`
- Remaining after this batch: 45

Batch G is the first window of the post-1.100 era, and the split is the
opposite of batch F: eleven of twenty commits are `C`, but only four of
those fail on philosophy. Five touch a sandbox this tree does not carry
(`crates/sandbox`, `crates/sandbox_linux`), one is release-workflow
plumbing, one is the guild compliance tool, and two depend on GPUI
animation APIs (`with_steps`, `with_max_fps`) that the local
`Animation` element never grew. The completion work is the batch's real
yield: three of the five `B` ports harden the remote completion path.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                           |
| ---------- | ----- | ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| 3a5180211a | B     | 39cb65eb90   | Silences Cargo's new `unused_dependencies` lint. Payload below.                                                                                       |
| 0ad670e6ea | C     | --           | Snapcraft release plumbing; `after_release.yml` and `release_nightly.yml` do not exist here and local CI is a hand-written `ci.yaml`.                 |
| 5dea5cab32 | B     | ff20164d9e   | Installs the rustls default provider before building the octocrab client. Payload below.                                                              |
| 42033befe6 | B     | 61b5b08fc2   | Skips the settings teardown when a worktree has nothing to clear. Payload below.                                                                      |
| de3827f6e9 | C     | --           | Keeps local settings of remote worktrees across a host reconnect; `crates/call`, `crates/collab` and `call.proto` are absent.                         |
| 10113795df | C     | --           | Adds a per-message `MessageContent::ReasoningEffort` variant and 21 provider files; local has no `crates/anthropic`, `open_ai` or `language_models`.  |
| c31063a050 | C     | --           | Mechanical `trim_{prefix,suffix}` adoption across 59 files; local already adopted it in `103e2ae596`, and the pick conflicted in eight deleted paths. |
| d9089f4eac | C     | --           | Re-renders spinner labels by glyph change; needs `Animation::with_steps`, which local `Animation` does not have.                                      |
| d46358213a | C     | --           | Fixes network checks in `crates/sandbox_linux`, which does not exist here.                                                                            |
| 1a391ced25 | B     | 69a1666753   | Carries the completion trigger context to the remote host. Payload below.                                                                             |
| 6501df1576 | A     | --           | Already equivalent: local `acp_thread` already writes through `Terminal::write_output`, and has no v2 display-terminal API.                           |
| 5a63e1a9e3 | A     | --           | Already equivalent: local `HitboxId` is already gated on `any(test, feature = "test-support")`.                                                       |
| 5e40758731 | C     | --           | Native Windows AppContainer sandbox; `crates/sandbox` is absent and the native-agent sandbox surface is rejected.                                     |
| a4227490e1 | A     | --           | Already equivalent: local `request_layout` already clamps `animation_ix` to `len().saturating_sub(1)`.                                                |
| 8057df9a20 | C     | --           | Schedules chained animations by their frame cap; the `with_max_fps`/`with_steps`/`step_wake` machinery it edits does not exist locally.               |
| 90549abcfd | C     | --           | Keeps layout nodes between frames; local `taffy.rs` is 1456 lines against a 787-line pre-image and the pick conflicted in eleven hunks.               |
| e224d55b90 | B     | 20ea0ab2e4   | Keeps completion edits anchored across concurrent buffer edits. Payload below.                                                                        |
| 4aa53044fd | C     | --           | Adds a `cargo test -p gpui --lib --no-run` step to `run_tests.yml` and its xtask twin; neither workflow exists locally.                               |
| 58fea3d335 | A     | 3195e4ab05   | Clean cherry-pick; the Windows font collection is no longer refreshed synchronously at startup.                                                       |
| 831229fe70 | C     | --           | Allows LAN connections in `crates/sandbox_linux`, which does not exist here.                                                                          |

Totals: four `A` (one landed, three already equivalent), five `B`,
eleven `C`.

## Applied Work

- `58fea3d335` used `git cherry-pick -x -s` against a clean tree and kept
  the upstream message.
- `42033befe6`, `5dea5cab32`, `1a391ced25` and `e224d55b90` started as
  cherry-picks, conflicted, and were committed with the port payload.
- `3a5180211a` was ported by hand after its toolchain hunks turned out to
  be already local.
- `6501df1576`, `5a63e1a9e3` and `a4227490e1` needed no code change.

No named upstream remote, push, or pull request was created.

## Per-Commit Narrative

### 3a5180211a — Rust 1.100 (B)

The toolchain move already landed as `103e2ae596`, so the pin, the CI
matrix and the `trim_prefix` rewrites had no work left. What did remain is
the `[workspace.lints.cargo] unused_dependencies = "allow"` table: the
lint is live under 1.100 and already reports
`crates/editor`'s production-only `assets` dependency as unused in a
test-support build. The port keeps it with the local tool annotated
(`cargo-machete`, not `cargo-shear`). `Dockerfile-collab`,
`crates/eval_cli` and `flake.lock` do not exist locally.

### de3827f6e9 — local settings across a host reconnect (C)

The invariant is "an existing remote worktree's local settings survive a
project update". Landing the worktree-creation clear alone did not hold
it: with the clear applied and the new test adapted to the local
`.ZZZ` settings directory, the second assertion of
`test_remote_local_settings_survive_new_worktree` failed at `(4, false)`
against `(2, true)` — creating a second remote worktree reset the first
one's editorconfig-derived settings. Restoring that requires the
`ProjectClientState::Collab { host_reconnected }` resync,
`SettingsObserver::shared` and the `call.proto` field, whose crates
(`crates/call`, `crates/collab`) are absent. Reverted in full; the
working tree was left clean.

### 10113795df — per-message reasoning effort (C)

`MessageContent::ReasoningEffort` is a new public request variant whose
only consumers are the provider crates this tree does not carry:
`crates/anthropic`, `crates/open_ai`, `crates/google_ai`,
`crates/language_models` and `crates/edit_prediction_cli` are all absent,
and `crates/copilot_chat` is rejected by policy. Nineteen of the
twenty-two files have no local host, and the remaining `language_model_core`
variant would have no caller.

### c31063a050 — `trim_{prefix,suffix}` adoption (C)

Local adopted these rewrites in `103e2ae596` as part of the 1.100 move.
This follow-up covers 59 files, and the pick conflicted in eight
modify/delete paths alone — `crates/zeta_prompt`, `crates/edit_prediction_cli`,
`crates/editor/src/input.rs`, `tooling/xtask/src/tasks/gpui/publish_plan.rs` —
plus content conflicts in `mention.rs`, `thread_view.rs`,
`docs_preprocessor`, `clone.rs`, `project_panel.rs` and others. A
mechanical sweep that has to be re-derived file by file on a diverged tree
is not an isolatable port.

### d9089f4eac and 8057df9a20 — animation steps and frame caps (C)

Both are expressed against `Animation::with_steps`, `Animation::with_max_fps`,
`AnimationState::step_wake` and `delayed_frame_pending`. Local
`crates/gpui/src/elements/animation.rs` is a 354-line element whose
`Animation` exposes only `new`, `repeat`, `repeat_synced` and
`with_easing`, so there is no state to thread a step clock or a frame cap
through. Restoring them means importing the unabsorbed animation work,
not porting this batch.

### d46358213a and 831229fe70 — `sandbox_linux` (C)

Both edit `crates/sandbox_linux/src/net.rs` and `policy.rs`, which this
tree does not have. No local caller exists for the sandbox surface.

### 1a391ced25 — completion trigger context over the wire (B)

Retained: `proto::GetCompletions` carries `trigger_character`, so a remote
buffer keeps `CompletionTriggerKind::TRIGGER_CHARACTER` instead of
degrading to `INVOKED` on the host, and the host recomputes the context
per server through `LspStore::completion_context_for_server` before it
dispatches. The editor also clears its stale completion tasks when a
reused menu already sits at the requested position.

Omitted: nothing behavior-bearing. Upstream's one-line
`editor/src/completions.rs` change was applied to
`crates/editor/src/editor.rs`, which is where local keeps that code, and
`completion_context_for_server` takes three arguments here rather than
four. The remote test was appended rather than merged in place, because
the local file lacks upstream's neighbouring `test_remote_call_hierarchy`.

### 6501df1576 — v2 terminal line endings (A, already equivalent)

Local `acp_thread` writes every display-terminal byte through
`Terminal::write_output` (`acp_thread.rs:4290`, `:4309`), which is the
state upstream arrives at. There is no `acp_v2` display terminal and no
`write_raw_output` locally, so the patch's own subject is absent.

### 5a63e1a9e3 — gpui unit tests without test-support (A, already equivalent)

`crates/gpui/src/window.rs:531` already gates `impl HitboxId` on
`#[cfg(any(test, feature = "test-support"))]`; the pick was a no-op.

### 5e40758731 — Windows AppContainer sandbox (C)

Introduces `crates/sandbox/src/windows_psec.rs` and deletes the WSL
sandbox. `crates/sandbox` does not exist here; the native-agent sandbox is
a rejected surface, and the commit also rewrites
`crates/settings_content/src/agent.rs` and the sandboxing docs around it.

### a4227490e1 — shorter animation chains (A, already equivalent)

The panic is guarded by clamping the animation index, and local
`request_layout` already does it more defensively at
`crates/gpui/src/elements/animation.rs:172`:
`state.animation_ix.min(self.animations.len().saturating_sub(1))`.

### 90549abcfd — layout nodes across frames (C)

The production change rewrites the Taffy engine's node handling — a
`DefaultKey` node cache, style matching, eviction — and adds six tests on
`crates/gpui/src/randomized_element_tree.rs` plus
`script/gpui-bench-compare`, both of which this tree deleted. Local
`taffy.rs` is 1456 lines with its own `absolute_outer_origins` and
`computed_layouts` state against a 787-line pre-image; the pick conflicted
in eleven hunks and its invariant could not be tested after the harness
was dropped.

### e224d55b90 — stale completion positions (B)

Retained: `GetCompletions` carries an `Anchor` instead of a
`PointUtf16`, so a response that arrives after the buffer moved is
validated against the text the request was made in. The new
`RequestedCompletionToken` records the request version, prefix and
separator, and `retain_valid_completions` drops the completions whose
fallback range no longer applies while marking the response incomplete.
`completion_token_range` and `completion_text_edit` are shared by the
filter, the fallback path and the proto round-trip.

Omitted: nothing behavior-bearing. `to_proto`/`from_proto` keep the local
`Result` signature, and the local per-server context loop replaces
upstream's hoisted request. Two test adaptations: the `oneshot` crate is
not available in `crates/project`, so the two new tests use
`futures::channel::oneshot`; and the `LanguageConfigOverride`-driven
`after_reparse` test is dropped because ZZZ has no language overrides.
`dynamic_registration.rs` stays absent — local never carried that module,
so its one-line call-site update had no host.

### 4aa53044fd — gpui unit tests in CI (C)

Adds one step to `.github/workflows/run_tests.yml` and its generated xtask
counterpart. Neither file exists locally; `ci.yaml` is hand-written and
its test job runs the whole workspace under nextest, where gpui's lib
tests are compiled with `test-support` unified in. Giving the invariant a
local host is a CI design decision, not a port of this commit.

## Verification

| Check                                                                                    | Result  | Notes                                                                                                  |
| ---------------------------------------------------------------------------------------- | ------- | ------------------------------------------------------------------------------------------------------ |
| `git diff --check` over the batch                                                        | PASS    |                                                                                                        |
| `cargo fmt --all -- --check`                                                             | PASS    | After `rustfmt` on the two ported files.                                                               |
| `cargo check --locked -p editor -p project -p remote_server -p settings`                 | PASS    | After the `42033befe6` and `1a391ced25` ports.                                                         |
| `cargo check --locked -p project -p remote_server --all-targets`                         | PASS    |                                                                                                        |
| `cargo check --locked --workspace`                                                       | PASS    | Warnings only; the `editor` unused-dependency warning is now silenced by the `3a5180211a` port.        |
| `cargo test --locked -p project --test integration test_completions_without_edit_ranges` | PASS    | 3 passed, including the two ported regression tests.                                                   |
| `cargo test --locked -p remote_server remote_completion`                                 | PASS    | 3 passed: `test_remote_completion_context`, `..._with_concurrent_edit`, `..._resolve_edit_ranges`.     |
| `cargo test --locked -p remote_server test_remote_local_settings_survive_new_worktree`   | FAIL    | Only for the reverted `de3827f6e9` experiment; the commit was dropped and the test is not in the tree. |
| `./script/backfill-upstream-ledger` and `cargo xtask check upstream-ledger`              | PASS    | Ledger regenerated from this report's decision table.                                                  |
| Prettier over this report                                                                | PASS    | `3.9.10`, the version pinned by `script/prettier`.                                                     |
| macOS, Windows, iOS and sandbox runtime behavior                                         | NOT RUN | This host is Linux. The Windows font, AppContainer and macOS hunks were inspected only.                |
