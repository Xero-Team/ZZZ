---
title: Upstream Sync 2026-10-21
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-10-21

## Scope

- Target branch: `sync/upstream-2026-10-21`, branched from `main` at
  `37438548bd`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `c4a82ab1ab958aa43b0c0b15c0bad4fb77a7ca7f`
- Reviewed head: `5adae449a366fd8f8f87d983287d95eab37e9488`
- Live upstream head queried: `5adae449a366fd8f8f87d983287d95eab37e9488`
- Query time: `2026-10-21T17:23:00+02:00`
- Reviewed range: every commit after `c4a82ab1ab`, oldest first
- Remaining after this batch: 0 commits through the queried live head

The previous reviewed baseline is an ancestor of `FETCH_HEAD`. The range
contains 26 commits, so the whole visible range fits in this batch.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                  |
| ---------- | ----- | ------------ | -------------------------------------------------------------------------------------------------------------------------------------------- |
| 294169aeed | C     | --           | Patches the OpenCode language-model provider, which ZZZ does not carry.                                                                      |
| 1e9923014d | B     | 7e1fa1e3fb   | Saves configuration defaults only after the agent accepts, using the response value. ZZZ's secondary-click toggle keeps its explicit write.  |
| 3c8e3b5a01 | C     | --           | Changes `agent_servers/src/acp/v1.rs` and `v2.rs`; ZZZ has a flat v1 `acp.rs` and no v2 session state.                                       |
| 3c11d49cbf | C     | --           | Needs the structured `DiffPatch`/`PatchFile` parser that ZZZ's simplified `acp_thread/src/diff.rs` does not carry.                           |
| a4a3a968d1 | C     | --           | Rewrites `crates/path`'s `PathStyle`; ZZZ has no `path` crate and its shared `util::paths` call sites do not need this refactor.             |
| b0f1e1cb2f | C     | --           | Preserves receipt sessions across authentication, which depends on the rejected ACP v2 receipt transport.                                    |
| 065d7ae597 | B     | 58fb00da32   | Ports `fuzzy_nucleo` search for model and configuration-option pickers. The benchmark crate and runner are omitted.                          |
| 92e8e2b8a0 | C     | --           | Pure upstream refactor to prepare extension-development work; it moves APIs into `extension` without changing ZZZ behavior.                  |
| d94887e08e | B     | ca64cf138d   | Reports both sides of a conflicting app borrow in GPUI panic messages.                                                                       |
| e70e0250a0 | C     | --           | Shuts down ACP v2 connections on teardown; ZZZ has no v2 connection module or runtime teardown path.                                         |
| 1d8a29e1e2 | C     | --           | Patches `crates/google_ai`, which ZZZ does not carry.                                                                                        |
| fae8a35482 | C     | --           | Selects ACP v2 for beta connections and extends v2 debug logging; the v2 architecture is absent.                                             |
| 92447a17cc | C     | --           | Lockfile-only `certifi` bump for the upstream issue-ranking helper, with no independent ZZZ behavior.                                        |
| 9f03ff88e4 | B     | 51fab18479   | Resolves notebook cell injections and re-parses on language changes. Adds a local `json_lang` test helper.                                   |
| d7adf3e7e2 | B     | 591344b5f3   | Adds the `extension` project-panel sort order with util and panel tests.                                                                     |
| 4c7ce998ad | C     | --           | Attaches ACP v2 permission requests to tool calls; ZZZ has no v2 permission model.                                                           |
| 3762891a55 | C     | --           | Fixes structured content in the OpenAI-compatible stream mapper in `language_model_core/src/chat_completion.rs`, which ZZZ does not carry.   |
| 70277c0193 | C     | --           | Splits agent-reported activity from prompt submission; ZZZ has no `submission.rs`, receipt transport, or `SessionActivity`.                  |
| 73a6e0ba61 | A     | 70dfbc3c0d   | Clean absorption: tail proximity bonus in `fuzzy_nucleo` path matching plus the file-finder regression test.                                 |
| f91e908cad | C     | --           | Parent-owned subagent registries depend on the absent ACP v2/receipt thread ownership and native subagent session machinery.                 |
| d38a754e1b | C     | --           | Adds ACP v2 session deletion on top of the absent v2 session state machine.                                                                  |
| 1416404b88 | C     | --           | Seven-hundred-line rework of discard selection, trash-untracked filtering, git transport, and the git panel; not isolatable onto ZZZ's flow. |
| a484d108da | C     | --           | Renders subagent cards from state introduced by the rejected `f91e908cad` subagent registry.                                                 |
| 41e9dc5837 | C     | --           | Shares OpenCode catalog parsing and completion dispatch; ZZZ has no `opencode` crate.                                                        |
| f26375b83b | C     | --           | Omits an OpenCode thinking block binding in the absent `crates/opencode`.                                                                    |
| 5adae449a3 | C     | --           | Fixes duplicated reasoning in the OpenAI-compatible stream mapper that ZZZ's `language_model_core` does not contain.                         |

Totals: one `A`, five `B`, twenty `C` across twenty-six commits.

## Applied Work

- `70dfbc3c0d` is a direct `git cherry-pick -x -s` of `73a6e0ba61`.
- `51fab18479` is a `B` port of `9f03ff88e4`.
- `7e1fa1e3fb` is a `B` port of `1e9923014d`.
- `591344b5f3` is a `B` port of `d7adf3e7e2`.
- `ca64cf138d` is a `B` port of `d94887e08e`.
- `58fb00da32` is a `B` port of `065d7ae597`.

## Per-Commit Narrative

#### 294169aeed — OpenCode model discovery (C)

The commit reads the npm package directory to discover models when `npm`
is omitted. ZZZ removed the OpenCode provider and its
`language_models/src/provider` tree, so there is no local call site.

#### 1e9923014d — configuration defaults after acceptance (B)

Cycling, boolean switches, and picker confirmation now share
`set_config_option_and_default`, which awaits the agent's response and
saves the response's current select or boolean value. Rejections,
removed options, unsupported kinds, and select values missing from the
response leave preferences untouched. ZZZ's secondary-click default
toggle still writes its explicit choice directly and does not save the
response value. Upstream's deferred-response test harness is absent, so
a unit test covers response-based default extraction instead.

#### 3c8e3b5a01 — ACP config defaults for new sessions (C)

The patch edits `agent_servers/src/acp/v1.rs` and `v2.rs` and extends the
v2 session builder. ZZZ has a single flat `agent_servers/src/acp.rs` that
implements v1 only.

#### 3c11d49cbf — Git-quoted patch paths (C)

Upstream normalizes Git C-quoted filenames and matches relative patch
paths against structured file changes. ZZZ's `acp_thread/src/diff.rs` is
a smaller buffer-diff renderer with no `DiffPatch`, `PatchFile`, or
`acp_v2::DiffFileType` parsing layer, so the change cannot be isolated.

#### a4a3a968d1 — component-based path normalization (C)

Replaces `PathStyle::normalize` in `crates/path` with
`normalize_separators` and `normalize_lexically` and migrates call sites
that ZZZ does not have. ZZZ's `util::paths` keeps the handwritten
`normalize`; porting would rewrite a shared API for upstream's upcoming
composable-join work with no local behavior change.

#### b0f1e1cb2f — receipt sessions during authentication (C)

Retains receipt-backed sessions while authenticating. ZZZ's `AcpThread`
has no receipt transport or pending submission recovery, so the retained
state cannot be represented.

#### 065d7ae597 — model and configuration search (B)

`model_search::search` builds name and metadata candidates, runs
`fuzzy_nucleo::match_strings_async` on the background executor, ranks name
matches first, and appends metadata-only matches without duplicates. Both
pickers use it, so punctuation differences and provider/group,
description, and value-id queries match, and the 100-result cap is gone.
The `agent_ui_benchmarks` crate, `script/bench-agent-model-search`, and
their lockfile entry are omitted as upstream benchmark plumbing.

#### 92e8e2b8a0 — extension resources move (C)

Moves manifest-default population and resource discovery into the
`extension` crate so upstream can align local and released extension
behavior later. It is a pure refactor with a new `extension_resources`
surface and no ZZZ behavior change; importing it would be scaffolding.

#### d94887e08e — app borrow origins (B)

`AppCell` records the first borrow location and kind in a group and
includes both the attempted call site and the existing group origin in
the conflicting-borrow panic. The regression test covers overlapping
shared borrows and origin replacement after unwinding.

#### e70e0250a0 — shut down ACP v2 connections (C)

Adds v2 teardown to `agent_servers/src/acp/runtime.rs`, `acp/v2.rs`, and
the connection store. ZZZ has none of those modules.

#### 1d8a29e1e2 — Google AI invalid keys (C)

Classifies invalid API keys as authentication errors in
`crates/google_ai`, which ZZZ does not carry.

#### fae8a35482 — select ACP v2 for beta (C)

Routes beta connections through the v2 builder and adds v2 debug logging.
ZZZ's flat v1 ACP module has no v2 builder or notification lanes.

#### 92447a17cc — certifi lock bump (C)

Updates one `uv.lock` entry in the upstream top-issues ranking helper.
This is lockfile-only maintenance for a triage script, with no
independent ZZZ behavior.

#### 9f03ff88e4 — notebook cell injections (B)

Cell buffers now receive the language registry, and each cell keeps a
task subscribed to language-registry changes that re-parses unknown
injections. Markdown and code cells highlight fenced and inline
languages immediately and after languages load. Upstream's shared
`language::test_language` helper is absent, so ZZZ adds a local
`json_lang` test-support helper and enables the optional
`tree-sitter-json` feature for the regression tests.

#### d7adf3e7e2 — extension sort order (B)

`project_panel.sort_order` accepts `extension`. Files group by their
lowercased last suffix in ascending order; extensionless files and
dotfiles come first, natural name order applies inside a group, and
directories participate as having no extension so `mixed` mode stays
transitive. Includes util unit tests, a panel test, a benchmark entry,
settings, and docs. ZZZ's util tests use `RelPath::unix` and the panel
test uses ZZZ's `ProjectPanel::new` call shape.

#### 4c7ce998ad — ACP v2 permissions on tool calls (C)

Matches v2 permission requests to tool calls. ZZZ has no v2 permission
model or permission-request store.

#### 3762891a55 — structured chat-completion content (C)

Adds `ResponseContent`, `ResponseContentPart`, and `ResponseThinkingPart`
to `language_model_core/src/chat_completion.rs`. That module does not
exist in ZZZ, and no local provider uses the mapper.

#### 70277c0193 — activity vs. submission (C)

Introduces `SubmissionTransport`, agent-owned sessions, and separate
foreground activity state. ZZZ has no `submission.rs`, no
`SessionActivity`, and no receipt transport to split.

#### 73a6e0ba61 — tail proximity bonus (A)

Adds a small reward for matches closer to the end of the path so
`x/x.py` outranks `y/x.py` for `x`, with the file-finder regression test.
The patch applied cleanly.

#### f91e908cad — parent-owned subagent threads (C)

Makes `AcpThread` own child sessions and rewires the native agent's
subagent registration. ZZZ's `AcpThread` still exposes
`subagent_spawned` and lacks the v2 session ownership and receipt
machinery this refactor builds on.

#### d38a754e1b — ACP v2 session deletion (C)

Implements session deletion in `agent_servers/src/acp/v2.rs`. The file
and its session state are absent.

#### 1416404b88 — keep staged changes when discarding (C)

Reworks discard targets and selection scope in the git panel, adds
`RevertedUnstaged`, threads an optional checkout commit through git and
the remote protocol, and filters trash-untracked files. The patch
conflicts across the git panel's selection, revert, trash, and test
flows, which have diverged locally; porting the whole rework is not an
isolatable B change, and the backend option alone would not change
behavior without the panel rewrite.

#### a484d108da — source-independent subagent cards (C)

Builds subagent card rendering on the state introduced by `f91e908cad`.
With that registry rejected, the card state has no local source.

#### 41e9dc5837 — OpenCode catalog sharing (C)

Moves catalog parsing and completion dispatch into `crates/opencode`,
which ZZZ does not carry.

#### f26375b83b — OpenCode thinking blocks (C)

Omits an unsupported thinking-block binding in the absent
`crates/opencode/src/completion.rs`.

#### 5adae449a3 — duplicated reasoning (C)

Adjusts the OpenAI-compatible stream mapper in
`language_model_core/src/chat_completion.rs`. ZZZ does not contain that
module or a local equivalent.

## Verification

| Check                                                                                    | Result  | Notes                                                 |
| ---------------------------------------------------------------------------------------- | ------- | ----------------------------------------------------- |
| Live `git ls-remote` and `git fetch --no-tags`                                           | PASS    | `FETCH_HEAD=5adae449a366fd8f8f87d983287d95eab37e9488` |
| Previous baseline ancestry                                                               | PASS    | `c4a82ab1ab` is an ancestor of `FETCH_HEAD`           |
| `cargo check --locked -p agent_ui -p util -p settings_content -p project_panel`          | PASS    | config defaults, search, sort order                   |
| `cargo check --locked -p gpui -p repl -p language -p fuzzy_nucleo -p file_finder`        | PASS    | borrow diagnostics and notebook injections            |
| `cargo test --locked -p repl --lib notebook::cell`                                       | PASS    | 3 tests                                               |
| `cargo test --locked -p file_finder --lib test_tail_proximity_bonus_prefers_later_match` | PASS    | 1 test                                                |
| `cargo test --locked -p gpui --lib app_borrow_diagnostics`                               | PASS    | 1 test                                                |
| `cargo test --locked -p util --lib compare_rel_paths_extension`                          | PASS    | 3 tests                                               |
| `cargo test --locked -p project_panel --lib test_sort_order_extension`                   | PASS    | 1 test                                                |
| `cargo test --locked -p agent_ui --lib config_option_search`                             | PASS    | 4 tests                                               |
| `cargo test --locked -p agent_ui --lib model_selector::tests`                            | PASS    | 13 tests                                              |
| `cargo test --locked -p agent_ui --lib setting_value_comes_from_authoritative_option`    | PASS    | 1 test                                                |
| `cargo fmt --check`                                                                      | PASS    | clean                                                 |
| `git diff --check 37438548bd..HEAD`                                                      | PASS    | clean                                                 |
| `./script/check-philosophy`                                                              | PASS    | philosophy gate                                       |
| Locale key parity (`en.json` vs `zh-CN.json`)                                            | PASS    | unchanged; no new keys                                |
| Windows and macOS builds                                                                 | NOT RUN | host is Linux                                         |
| `cargo test --workspace`                                                                 | NOT RUN | not requested                                         |

The general reviewed baseline advances to
`5adae449a366fd8f8f87d983287d95eab37e9488`.
