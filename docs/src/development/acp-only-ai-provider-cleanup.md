---
title: ACP-only Provider Stack Cleanup Plan
description: Removal plan for in-tree model providers, the native agent's model runtime, and non-ACP AI surfaces.
---

# ACP-only Provider Stack Cleanup Plan

This is the second pass of the ACP-only AI cleanup. It follows
[ACP-only AI Cleanup Plan](./acp-only-ai-cleanup.md), which removed the dead
`web_search` registry and the `edit_prediction` stack. This pass removes the
provider tree, the native agent's model runtime, and the non-ACP AI features
that still call a model directly.

This is a large pass. Unlike the first one, it changes user-visible behavior:
features that had their own model picker and request path stop existing. Treat
this document as guidance, not a script. Every phase ends with a
`cargo check --workspace` gate; do not proceed while it fails.

> **Warn:** This pass is expected to break `cargo test --workspace`
> throughout. That is accepted while the removal is in progress. Tests are
> repaired or deleted at the end, not per phase.

## Goal {#goal}

After this pass:

- No crate under `crates/` calls a model provider. All model calls come from
  an external ACP agent.
- ZZZ keeps the ACP client surface (`acp_thread`, `agent_servers`,
  `acp_tools`), the elicitation UI, agent-server configuration, and the
  credential/env plumbing an external agent needs to launch.
- Native-agent-only and single-shot AI features are deleted, not stubbed.
  This includes the inline assistant, buffer and terminal codegen, generated
  commit messages, and generated thread titles.

## What must survive {#what-survives}

The provider tree is entangled with ACP. These pieces are load-bearing and
must be slimmed rather than deleted:

| Crate / symbol                             | Consumer and reason                                                                |
| ------------------------------------------ | ---------------------------------------------------------------------------------- |
| `language_model::{ApiKey,EnvVar}`          | `crates/agent_servers/src/custom.rs:8` launches external agents (Gemini env var).  |
| `language_model::LanguageModelProviderId`  | `crates/acp_thread/src/connection.rs:7` records `AuthRequired::provider_id`.       |
| `prompt_store`                             | `crates/acp_thread/src/mention.rs` builds ACP mentions.                            |
| `cloud_api_types`                          | `extension`, `extension_host`, `extensions_ui`, `title_bar` all depend on it.      |
| `agent::{ThreadStore,db,outline}`          | Shared ACP thread storage; `agent_ui` uses it for thread metadata and the archive. |
| `acp_thread`, `agent_servers`, `acp_tools` | The ACP client itself.                                                             |

Two consequences follow from the table:

- `language_model` cannot be deleted outright. It shrinks to the auth and
  identity types ACP needs, or those types move into a smaller crate.
- `cloud_llm_client` cannot be deleted while `cloud_api_types` depends on it
  (verified: `crates/cloud_api_types/Cargo.toml:18`). The dependency must be
  severed first.

`language_model_core` exports small enums that `settings_content` consumes
(`Speed`, `ReasoningEffort`, `ModelMode`). Inline those into
`settings_content` before dropping the crate.

## Inventory {#inventory}

### Provider registry and implementations (delete)

| Crate             | Role                                                        |
| ----------------- | ----------------------------------------------------------- |
| `language_models` | Provider registry, settings, extension proxy, models UI     |
| `anthropic`       | Anthropic provider HTTP client and types                    |
| `open_ai`         | OpenAI provider                                             |
| `google_ai`       | Google AI provider (`API_URL` also used by `agent_servers`) |
| `open_router`     | OpenRouter provider                                         |
| `opencode`        | OpenCode Zen provider                                       |
| `mistral`         | Mistral provider                                            |
| `deepseek`        | DeepSeek provider                                           |
| `lmstudio`        | LM Studio provider                                          |
| `ollama`          | Ollama provider                                             |
| `llama_cpp`       | llama.cpp provider                                          |
| `bedrock`         | AWS Bedrock provider                                        |
| `x_ai`            | xAI provider                                                |

`language_models` is the hub: it registers every provider in
`crates/language_models/src/language_models.rs:35` (`init`) and
`:241`–`:372` (`register_providers`), and it owns
`AllLanguageModelSettings`, the extension provider proxy
(`crates/language_models/src/extension.rs`), and the Copilot Chat provider.

### Copilot (delete)

| Crate          | Role                                                 |
| -------------- | ---------------------------------------------------- |
| `copilot`      | GitHub Copilot auth, binary download, agent endpoint |
| `copilot_ui`   | Copilot sign-in and configuration views              |
| `copilot_chat` | Copilot Chat language model client                   |

Copilot survives the first pass because it is still a provider for the native
agent. It goes here.

### Cloud model client (delete or refactor)

| Crate              | Role                                                          |
| ------------------ | ------------------------------------------------------------- |
| `cloud_llm_client` | Zed-hosted model client (`predict_edits_v3` already removed)  |
| `cloud_api_types`  | Cloud API types; keep, but remove the `cloud_llm_client` edge |

`cloud_api_types` is still required by extensions and the title bar. Only the
`cloud_llm_client` dependency
(`crates/cloud_api_types/Cargo.toml:18`,
`crates/language_model_core/Cargo.toml:18`) is removed.

### Native agent model runtime (delete parts of `crates/agent`)

`crates/agent` mixes two roles. Keep the shared storage and delete the model
runtime:

- Delete: `legacy_thread.rs`, `history.rs`, `tool_protocol.rs`, `tools.rs`,
  `tools/`, `pattern_extraction.rs`, and the LLM-dispatch code in `agent.rs`
  that uses `language_models`.
- Keep: `thread_store.rs`, `db.rs`, `outline.rs`, and the persistence used by
  ACP threads. `db.rs` and `history.rs` currently serialize
  `language_model` message types; decide whether to keep those types as a
  storage DTO or migrate the schema before deleting them.

`cloud_llm_client` is used only by this runtime plus `cloud_api_types`, so it
disappears with it.

### Non-ACP single-shot AI surfaces (delete)

These call a model outside an ACP conversation:

| Location                                             | Feature                          |
| ---------------------------------------------------- | -------------------------------- |
| `crates/agent_ui/src/inline_assistant.rs`            | Inline assistant                 |
| `crates/agent_ui/src/terminal_inline_assistant.rs`   | Terminal inline assistant        |
| `crates/agent_ui/src/inline_prompt_editor.rs`        | Inline assist prompt editor      |
| `crates/agent_ui/src/buffer_codegen.rs`              | Buffer codegen action            |
| `crates/agent_ui/src/terminal_codegen.rs`            | Terminal codegen action          |
| `crates/agent_ui/src/agent_configuration*.rs`        | In-tree provider configuration   |
| `crates/agent_ui/src/language_model_selector.rs`     | Legacy model picker              |
| `crates/agent_ui/src/agent_model_selector.rs`        | Legacy agent model selector      |
| `crates/agent_ui/src/favorite_models.rs`             | Favorites for the above          |
| `crates/git_ui/src/git_panel.rs`                     | Generated commit messages        |
| `crates/rules_library/src/rules_library.rs`          | Rule generation via inline model |
| `crates/settings_ui/src/pages/llm_providers_page.rs` | Settings for local providers     |

Keep `crates/agent_ui/src/model_selector.rs`, `model_selector_popover.rs`,
`mode_selector.rs`, `config_options.rs`, `profile_selector.rs`, and
`agent_registry_ui.rs`: these drive ACP agents, not in-tree providers. Verify
each against `acp_thread` before deleting.

## Dependency breakpoints {#breakpoints}

Verified with `rg -l "^<crate>( =|\.workspace| = \{) " crates/*/Cargo.toml`:

| Dependent                               | Depends on                                                                                            |
| --------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `language_models`                       | all provider crates, `copilot`/`copilot_ui`/`copilot_chat`, `language`                                |
| `zzz`                                   | `agent`, `copilot`, `copilot_chat`, `copilot_ui`, `language_model`, `language_models`, `prompt_store` |
| `agent`                                 | `language_model`, `language_models`, `cloud_llm_client`, `cloud_api_types`, `prompt_store`            |
| `agent_ui`                              | `language_model`, `language_models`, `cloud_api_types`, `prompt_store`                                |
| `agent_settings`                        | `language_model`                                                                                      |
| `git_ui`                                | `language_model`                                                                                      |
| `rules_library`                         | `language_model`                                                                                      |
| `settings_ui`                           | `copilot`, `copilot_ui`                                                                               |
| `language_model`                        | `language_model_core`                                                                                 |
| `language_model_core`                   | `cloud_llm_client`; `settings_content`                                                                |
| `cloud_api_types`                       | `cloud_llm_client`                                                                                    |
| `agent_servers`                         | `google_ai` (uses `google_ai::API_URL` in `custom.rs:292`)                                            |
| `sidebar`, `git_graph`, `remote_server` | `language_model`                                                                                      |

The awkward edges are the ones to plan around:

- `settings_content` -> `language_model_core` -> `cloud_llm_client`. Break
  this from the settings side first by inlining the small enums.
- `agent_servers` -> `google_ai`. Replace the `google_ai::API_URL` use with a
  local constant so `google_ai` can be deleted.
- `agent` -> `language_models`/`cloud_llm_client`. Delete the model runtime
  before removing the crates.

## Phases {#phases}

Order matters: unwire the UI first, decouple shared crates second, delete the
provider implementations third, and clean settings last.

### Phase A — Unwire user-visible AI surfaces

- `crates/zzz/src/main.rs:572`–`:574`, `crates/zzz/src/zzz.rs:5710`–`:5711`,
  `crates/zzz/src/visual_test_runner.rs:207`–`:208`: drop `copilot_ui::init`,
  `language_model::init`, and `language_models::init` calls when their
  consumers are gone.
- Remove the actions and menus for `assistant::InlineAssist`,
  `editor::Generate*`, and the codegen actions, and the corresponding keymap
  bindings and i18n keys.
- Delete the inline assistant, codegen, provider configuration, and legacy
  model-selector modules listed above.
- `crates/settings_ui`: remove the LLM providers page and its page data entry
  (`crates/settings_ui/src/page_data.rs:9144`–`:9167`).

### Phase B — Decouple shared crates

- `crates/agent`: delete the model runtime; keep `ThreadStore`, `db`,
  `outline`. Drop the `language_models` and `cloud_llm_client` deps. Replace
  the `language_model` message/tool types in `db.rs` and `history.rs` with
  storage DTOs, or delete the legacy history path if ACP owns persistence.
- `crates/agent_settings`: remove `LanguageModelSelection`-driven settings
  (`default_model`, `subagent_model`, `inline_assistant_model`,
  `commit_message_model`, `thread_summary_model`, `favorite_models`,
  `inline_alternatives`) and `language_model_to_selection`. Keep whatever
  agent-profile fields the ACP agent server needs.
- `crates/agent_servers/src/custom.rs:287`–`:295`: keep the Gemini env-var
  and keychain launch path. Replace `google_ai::API_URL` with a local
  constant.
- `crates/settings_content`: inline `Speed`, `ReasoningEffort`, `ModelMode`
  from `language_model_core`; delete `all_language_models`, provider-specific
  blobs, and the `language_models` settings field.
- `crates/language/src/language_settings.rs`: delete `EditPredictionSettings`
  and the remaining `edit_predictions` field.
- `crates/rules_library`, `crates/sidebar`, `crates/git_graph`,
  `crates/git_ui`: remove `LanguageModelRegistry` uses and the features they
  power.

### Phase C — Delete provider implementations

1. Delete the provider crates from the inventory table.
2. Delete `language_models` (registry, settings, extension proxy, models UI).
3. Remove the workspace members and dependencies, and
   `crates/language_models/src/extension.rs`'s extension-provider proxy if
   extensions should no longer register providers.
4. `cargo check --workspace`; every remaining error is a missed breakpoint.

### Phase D — Delete the Copilot stack

1. Delete `copilot`, `copilot_ui`, `copilot_chat` and their workspace
   entries.
2. Remove `copilot_ui::init` and the Copilot configuration views.
3. Remove Copilot settings and i18n keys.

### Phase E — Delete the cloud client and slim `language_model`

1. Remove the `cloud_llm_client` edge from `cloud_api_types` and
   `language_model_core`, then delete `cloud_llm_client`.
2. Delete provider-specific modules from `language_model_core`.
3. Reduce `language_model` to the auth and identity types ACP needs
   (`ApiKey`, `EnvVar`, `LanguageModelProviderId`), or move them into a small
   crate and delete `language_model`.

### Phase F — Settings, keymaps, i18n, docs, philosophy

- Remove `language_models`, `edit_predictions`, and provider settings from
  `assets/settings/default.json`.
- Remove `assets/keymaps/**` bindings for the deleted actions.
- Remove removed keys from `assets/locales/en.json` and
  `assets/locales/zh-CN.json`; keep the two catalogs in sync.
- Add a `crates/migrator` migration that strips the removed settings keys.
- Update user docs: `ai/inline-assistant.md`, `ai/llm-providers.md`,
  `ai/edit-prediction.md`, `completions.md`, `reference/all-settings.md`, and
  `SUMMARY.md`.
- Shrink `script/philosophy-allowlist` and tighten `script/check-philosophy`
  checks that reference provider crates.

## Removal order {#removal-order}

1. Phase A: unwire `agent_ui`, `git_ui`, `rules_library`, `settings_ui`.
2. Phase B: decouple `agent`, `agent_settings`, `settings_content`,
   `language`.
3. Phase C: delete provider crates and `language_models`.
4. Phase D: delete the Copilot stack.
5. Phase E: delete `cloud_llm_client`, slim `language_model_core` and
   `language_model`.
6. Phase F: settings, keymaps, i18n, docs, philosophy.

## Verification per phase {#verification}

| Check                   | Command                                |
| ----------------------- | -------------------------------------- |
| No dangling references  | `rg -n "<crate>" crates Cargo.toml`    |
| Type/borrow correctness | `cargo check --workspace`              |
| Lint                    | `./script/clippy`                      |
| Tests (final only)      | `cargo test --workspace`               |
| No new hosts introduced | `./script/check-philosophy`            |
| Docs formatting         | `cd docs && npx prettier --check src/` |

Tests are excluded until the end because the removal is expected to break
them. Run the full suite once, at Phase F, and either update or delete the
affected tests.

## Risks and open questions {#risks}

- ACP auth plumbing shares `language_model::{ApiKey, EnvVar}` and
  `LanguageModelProviderId`. Confirm what `agent_servers` needs at launch
  before deleting `language_model_core`.
- The extension ABI lets extensions register language model providers
  (`crates/language_models/src/extension.rs`). Decide whether external
  extensions may still supply providers, and if not, remove the proxy and the
  extension API surface in the same phase.
- `agent::db` persists native message types. Changing the schema affects
  existing user history; prefer a migration or keep the DTOs.
- `git_ui` generated commit messages and `agent_ui` thread summaries have no
  ACP equivalent. Removing them is a deliberate feature loss; confirm before
  deleting.
- `settings_content` and `vscode_import` reference language model settings;
  removing the schema requires a settings migration to avoid load errors.

## Out of scope {#out-of-scope}

- ACP agent-side changes. External agents keep their own model runtime.
- The elicitation question/answer bridge, tracked in
  `upstream-sync-2026-09-22-acp-elicitation.md`.
- Re-adding any of the removed AI features through ACP. That would be a new
  design, not a cleanup.
