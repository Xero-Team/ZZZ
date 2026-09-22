---
title: Upstream Sync 2026-09-22 ACP Elicitation
description: Targeted re-absorption of the skipped ACP elicitation work.
---

# Upstream Sync 2026-09-22 ACP Elicitation

## Scope

- Target branch: `sync/upstream-2026-09-22-acp-elicitation` from `main` at
  `43a035f8c015d3b8618da174a9ba7e06024c3822`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previously reviewed baseline: `b961b4950febbc050081554bafe976b5d1b93f39`
- Live upstream head queried: `16c9aa7ea6d897a8044d9501cde1b295256722f2`
- Query time: `2026-09-22T18:05:00+02:00`

This run is not a forward batch. It re-opens commits already inside the
reviewed range that were classified `C` because ZZZ pinned
`agent-client-protocol` at `=0.12.0`. The goal is the ACP elicitation client
surface; the OpenCode-side bridge is explicitly out of scope.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                       |
| ---------- | ----- | ------------ | ------------------------------------------------------------------------------------------------- |
| c413552859 | B     | 7035311d19   | SDK 0.13.1 migration; drop the session-model selector replaced by session config options.         |
| 56b71271c4 | B     | 7035311d19   | SDK 0.14.0; session usage/deletion already backported locally, only the version bump is retained. |
| 8ba35e5eac | B     | 7035311d19   | SDK 0.15.0 API migration across `acp_thread`, `agent_servers`, and `agent_ui`.                    |
| 9de0590a81 | B     | 7035311d19   | SDK 1.0.0.                                                                                        |
| b04ae5ed81 | B     | 7035311d19   | SDK 1.0.1.                                                                                        |
| f15a3ef452 | B     | 7035311d19   | SDK 1.3.0.                                                                                        |
| 984e3bd0ce | B     | 7035311d19   | SDK 2.0.0.                                                                                        |
| 3cef31688a | B     | 7035311d19   | SDK 2.1.0; the `futures` bump was not needed.                                                     |
| fa66442a01 | B     | 16d4c21c86   | Elicitation store, client handlers, capability, and session-scoped cards.                         |
| c05b439174 | B     | 16d4c21c86   | Elicitation option descriptions.                                                                  |
| fbceb2823b | B     | 16d4c21c86   | Enable ACP elicitations.                                                                          |
| 33e2058302 | B     | 16d4c21c86   | Elicitation warnings and validation.                                                              |
| 68ec865bfd | B     | 16d4c21c86   | Elicitation form keyboard navigation.                                                             |
| 8f92822cbf | B     | 16d4c21c86   | Only the `unicode_confusables` scanner needed by the elicitation URL warning is retained.         |
| 632d805d64 | C     | --           | Native agent `ask_user`; ZZZ keeps no native agent surface.                                       |
| 73ee8fa038 | C     | --           | Native agent `ask_user` option wrapping.                                                          |

## Applied work

### 7035311d19 — `sync: update agent-client-protocol SDK to 2.1.0`

Retained the local ACP surface on `schema::v1`, moved `ProtocolVersion` to the
schema root, converted `MessageId` at the session-update boundary, and kept
session config options as the model-selection path. The unstable session-model
selector was removed because upstream replaced it with session config options;
`AgentModelId` is now a local newtype and model favorites live on
`AgentModelSelector`. ZZZ's `AgentConfigOptionValue` boolean-config
abstraction is preserved.

### 16d4c21c86 — `sync: add ACP elicitation support from fa66442a01`

Added `ElicitationStore` to `acp_thread`, the session-scoped and
request-scoped client handlers plus the client elicitation capability to
`agent_servers`, `conversation_view/elicitation.rs`, and session-scoped
elicitation cards in the thread view. The `unicode_confusables` helper is
reduced to the scanner used by the elicitation URL warning.

## Verification

| Check                                                | Result  |
| ---------------------------------------------------- | ------- |
| `cargo check -p acp_thread`                          | PASS    |
| `cargo check -p agent_servers` (with `test-support`) | PASS    |
| `cargo check -p agent_ui`                            | PASS    |
| `cargo check -p zzz`                                 | PASS    |
| `cargo fmt --all`                                    | PASS    |
| `./script/clippy` (full release run)                 | PASS    |
| `cargo test` on touched crates                       | NOT RUN |
| macOS / Windows runtime checks                       | NOT RUN |

## Remaining

- Request-scoped elicitation cards in the conversation view are not wired;
  only the store and handlers exist.
- The new elicitation UI strings are not localized into the ZZZ locale
  catalogs.
- `unicode_confusables` is ported without the sandbox-prompt display helpers
  that upstream uses outside elicitation.
