---
title: Upstream Sync 2026-10-24 (c)
description: Selective Zed upstream sync audit, third batch.
---

# Upstream Sync 2026-10-24 (c)

## Scope

- Target branch: `sync/upstream-2026-10-24`, continuing from the second
  batch
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `27ea42bf01deeaca66ee778d5eaee1f336708847`
- Reviewed head: `79fe6a645f68f8c9230578f154bf99852d9f716f`
- Live upstream head queried: `79fe6a645f68f8c9230578f154bf99852d9f716f`
- Query time: `2026-10-24T00:56:10+02:00`
- Reviewed range: every remaining commit after `27ea42bf01`, oldest first
- Remaining after this batch: 0 commits through the queried live head

The previous reviewed baseline is an ancestor of `FETCH_HEAD`. The range
contains 11 commits, so the whole remaining range fits in this batch.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                      |
| ---------- | ----- | ------------ | -------------------------------------------------------------------------------------------------------------------------------- |
| e841324d0d | C     | --           | Removes unused GitHub action definitions. CI/release automation is out of scope.                                                 |
| 4f862ffbc6 | C     | --           | Deletes upstream workflows, prompt files, and docs/CI scripts. Community and docs automation is out of scope.                    |
| 2214d0a92e | B     | 4f3b1bfaa8   | Refreshes managed Delve on each debug session and falls back to the cached binary. ZZZ's binary-download gate is preserved.      |
| b36b283021 | C     | --           | Adds a credential-read gate across the language-model provider tree, which ZZZ does not carry.                                   |
| 1e1f5d7628 | C     | --           | Changes sandboxed terminal handling, but ZZZ's `acp_thread` has no `sandbox_wrap` path.                                          |
| 91d3b962ae | C     | --           | Swaps `oo7` to an upstream-maintained git fork. Dependency-only change with no independent ZZZ behavior.                         |
| e96ffb0fd3 | C     | --           | Shows ACP session messages through v2 session updates and the diverged transcript view; ZZZ has the simplified ACP thread model. |
| 604a0ff023 | C     | --           | Changes OpenAI chat-completion error handling in `crates/open_ai`, which ZZZ does not carry.                                     |
| 9f305aaf88 | C     | --           | Handles null macOS open/save panels on a diverged `gpui_macos`; cannot be verified on this Linux host.                           |
| abd9b3cbd2 | C     | --           | Changes `gpui_apple` keychain behavior; the Apple platform crate is absent locally.                                              |
| 79fe6a645f | C     | --           | Adds a macOS-only atomic credential write API with no current ZZZ caller and no Apple backend.                                   |

Totals: zero `A`, one `B`, ten `C` across eleven commits.

## Applied Work

- `4f3b1bfaa8` is a `B` port of `2214d0a92e`.

## Per-Commit Narrative

#### e841324d0d — remove GitHub action files (C)

Deletes `.github/actions/check_style`, `run_tests`, and
`run_tests_windows`. This is upstream CI automation with no local product
behavior.

#### 4f862ffbc6 — remove unused workflows and scripts (C)

Deletes upstream workflows, `.factory` prompts, docs-suggestion tooling,
triage scripts, and randomized test helpers. ZZZ keeps its own release and
documentation automation; importing this cleanup would only prune
upstream-only infrastructure.

#### 2214d0a92e — refresh managed Delve (B)

When no user-installed or `PATH` `dlv` exists, the adapter now re-runs
`go install ...@latest` on every debug session instead of reusing a stale
managed binary. A failed refresh falls back to the cached binary with a
warning. ZZZ's `allow_binary_download` check stays ahead of the install
attempt.

#### b36b283021 — skip credentials when AI is disabled (C)

Adds `CredentialsReadGate`, a `read_credentials` wait loop in
`language_model/src/api_key.rs`, provider updates across the absent
`language_models` and `copilot_chat` trees, and Settings UI changes. The
commit is a provider-architecture refactor; ZZZ has no central
`language_models` initialization point to install the gate, and most of the
touched providers do not exist.

#### 1e1f5d7628 — non-interactive sandboxed terminals (C)

Changes the `headless || (linux && sandbox_wrap.is_some())` condition in
`acp_thread`. ZZZ's ACP thread has no `sandbox_wrap` value, so there is no
local call site.

#### 91d3b962ae — patched oo7 branch (C)

Repoints `oo7` from crates.io 0.6.0 to a Zed-maintained git fork for a
Linux keyring panic fix. This is a dependency-only swap; the lockfile and
dependency source would be the only changes, with no independent ZZZ
behavior.

#### e96ffb0fd3 — ACP session messages in the transcript (C)

Adds session-message variants to `acp_thread`, v1/v2 session-update
handling, and transcript rendering in `agent_ui`. ZZZ carries the
simplified ACP thread without v2 session state, so the change cannot be
isolated onto the current model.

#### 604a0ff023 — typed OpenAI stream errors (C)

Reworks `crates/open_ai` chat-completion transport errors and the provider
wrappers around it. ZZZ does not carry the `open_ai` crate.

#### 9f305aaf88 — null macOS panels (C)

Adds null checks around `NSOpenPanel` and `NSSavePanel` in `gpui_macos`.
The local macOS platform layer has diverged and this Linux host cannot
compile or run it.

#### abd9b3cbd2 — Apple keychain prompt state (C)

Distinguishes missing credentials from a cancelled prompt in
`gpui_apple/src/keychain.rs`. The Apple platform crate is absent in ZZZ.

#### 79fe6a645f — write credentials only if absent (C)

Adds `write_credentials_if_not_exists` to GPUI, the credential provider
trait, macOS, and Apple keychain code. The GPUI and platform methods are
macOS-only, the Apple backend is absent, and no current ZZZ caller needs the
new API.

## Verification

| Check                                          | Result  | Notes                                                 |
| ---------------------------------------------- | ------- | ----------------------------------------------------- |
| Live `git ls-remote` and `git fetch --no-tags` | PASS    | `FETCH_HEAD=79fe6a645f68f8c9230578f154bf99852d9f716f` |
| Previous baseline ancestry                     | PASS    | `27ea42bf01` is an ancestor of `FETCH_HEAD`           |
| `cargo check --locked -p dap_adapters`         | PASS    | Delve refresh                                         |
| `cargo test --locked -p dap_adapters --lib go` | NOT RUN | no Go adapter tests exist                             |
| `cargo fmt --check`                            | PASS    | clean                                                 |
| `git diff --check main..HEAD`                  | PASS    | clean                                                 |
| `./script/check-philosophy`                    | PASS    | philosophy gate                                       |
| Locale key parity (`en.json` vs `zh-CN.json`)  | PASS    | unchanged                                             |
| macOS builds and runtime checks                | NOT RUN | host is Linux                                         |
| Windows builds and runtime checks              | NOT RUN | host is Linux                                         |
| `cargo test --workspace`                       | NOT RUN | not requested                                         |
| Remaining commits through live head            | NONE    | caught up with `79fe6a645f`                           |

The reviewed baseline advances to
`79fe6a645f68f8c9230578f154bf99852d9f716f`, the live upstream head at query
time.
