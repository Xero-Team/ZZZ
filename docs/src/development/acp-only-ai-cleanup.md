---
title: ACP-only AI Cleanup Plan
description: Removal plan for native-agent leftovers and non-ACP AI surfaces.
---

# ACP-only AI Cleanup Plan

This is an internal working plan for making all AI in ZZZ flow through ACP
external agents. That direction has two consequences:

- The native agent is not a feature to restore. It is code to remove.
- In-tree model providers are not a stack to maintain. They are code to remove.

This document covers the first, low-risk passes: the dead `web_search`
registry and the `edit_prediction` stack. The provider tree
(`language_model`, `language_models`, and the provider crates) is a later,
larger pass and is only sequenced here.

> **Note:** This plan is guidance, not a script. Each phase ends with a
> `cargo check --workspace` gate; do not proceed while it fails.

## Policy

- AI capability must come from an ACP agent (OpenCode, Claude Code, Gemini
  CLI, ...). ZZZ owns the ACP client surface and the elicitation form UI, not
  the model runtime.
- No crate under `crates/` may call a model provider directly after the
  cleanup completes, except the auth plumbing an external agent needs to
  launch.
- Anything reachable only from the removed native agent is dead code and
  should be deleted, not stubbed.

## Phase 0 — `web_search` (pure dead code)

### Evidence

- `crates/web_search_providers/src/web_search_providers.rs` is 6 lines and
  registers nothing:
  ```rust
  pub fn init(cx: &mut App) {
      let _registry = WebSearchRegistry::global(cx);
  }
  ```
- Nothing reads `WebSearchRegistry`, `WebSearchProvider`, or `active_provider`.
  The only near-match, `active_provider_id` in
  `crates/agent_ui/src/language_model_selector.rs`, is about model providers.
- `crates/agent/Cargo.toml:74` declares `web_search`, but `crates/agent/`
  contains no code reference. This is an orphaned dependency.
- No `WebSearchTool` exists anywhere; the tool that consumed the registry was
  removed with the native agent.
- `agent_servers` and `acp_thread` never reference `web_search`. ACP agents
  bring their own web search.

### Edits

1. Remove init calls and deps:
   - `crates/zzz/src/main.rs:603` and `:604`
   - `crates/zzz/src/zzz.rs:5737` and `:5739`
   - `crates/zzz/Cargo.toml:218` and `:219`
   - `crates/agent/Cargo.toml:74`
2. Remove workspace entries:
   - `Cargo.toml` members `crates/web_search` and
     `crates/web_search_providers`
   - `Cargo.toml` workspace dependencies `web_search` and
     `web_search_providers`
3. Delete `crates/web_search/` and `crates/web_search_providers/`.

### Verification

- `rg -n "web_search" crates Cargo.toml` returns nothing.
- `cargo check -p zzz -p agent`.

## Phase 1 — `edit_prediction` (feature removal)

Unlike `web_search`, this is a live feature. Removing it touches the editor,
settings, keymaps, i18n, and the status bar. Order matters: unwire the UI
first, decouple the editor second, delete crates last.

### Inventory

Crates to delete:

| Crate                     | Role                                   |
| ------------------------- | -------------------------------------- |
| `edit_prediction`         | store, providers, Zeta/FIM, udiff      |
| `edit_prediction_context` | prompt context assembly                |
| `edit_prediction_metrics` | metrics                                |
| `edit_prediction_types`   | shared types consumed by `editor`      |
| `edit_prediction_ui`      | status-bar button, menu, onboarding UI |
| `zeta_prompt`             | Zeta prompt templates                  |

### Dependency breakpoints

Current inward edges (verified with `rg -l "<crate>" crates/*/Cargo.toml`):

| Dependent         | Depends on                                                                       |
| ----------------- | -------------------------------------------------------------------------------- |
| `editor`          | `edit_prediction_types`                                                          |
| `settings_ui`     | `copilot`, `copilot_ui`, `edit_prediction`, `edit_prediction_ui`                 |
| `language_tools`  | `edit_prediction`                                                                |
| `zzz`             | `copilot`, `copilot_chat`, `copilot_ui`, `edit_prediction`, `edit_prediction_ui` |
| `edit_prediction` | `ollama`, `cloud_llm_client`, `language_model`                                   |
| `zeta_prompt`     | `edit_prediction_context`, `edit_prediction`, `cloud_llm_client`                 |

### 1a — Unwire the user-visible surface

- `crates/zzz/src/main.rs`
  - `:72` drop `edit_prediction_registry` from the import list
  - `:576` and `:591` remove the `edit_predictions` settings reads
  - `:602` remove `edit_prediction_ui::init(cx)`
  - `:606` remove `edit_prediction_registry::init(...)`
  - `:686` remove `edit_prediction::init(cx)`
- `crates/zzz/src/zzz.rs`
  - `:2` remove `pub mod edit_prediction_registry;`
  - `:548`–`:559` remove the status-bar `EditPredictionButton` wiring
  - `:606` remove `status_bar.add_right_item(edit_prediction_ui, ...)`
  - `:2210` and `:5482` remove `edit_prediction` from action/namespace lists
- Delete `crates/zzz/src/zzz/edit_prediction_registry.rs` (registers the
  Copilot/cloud delegate).
- `crates/zzz/Cargo.toml:128` and `:129` remove the deps.
- `settings_ui`: delete
  `crates/settings_ui/src/pages/edit_prediction_provider_setup.rs` and its
  page registration; remove deps from `crates/settings_ui/Cargo.toml`.
- `language_tools`: remove the `edit_prediction` use in
  `crates/language_tools/src/lsp_log_view.rs` (`EditPredictionStore` at `:2`,
  `set_show_edit_predictions` at `:1306`) and the dep at
  `crates/language_tools/Cargo.toml:21`.

### 1b — Decouple `editor`

`crates/editor/Cargo.toml:57` depends on `edit_prediction_types`. The editor
uses a small set of symbols:

- `EditPrediction` (payload; ~14 sites)
- `EditPredictionIconSet` (2)
- `EditPredictionDiscardReason` (2)

Referenced files: `split.rs`, `selection.rs`, `scroll.rs`, `movement.rs`,
`inlays.rs`, `element.rs`, `editor.rs`, `display_map.rs`,
`display_map/inlay_map.rs`, `diagnostics.rs`, plus
`edit_prediction_tests.rs`.

Choose one:

- **Preferred:** remove the prediction render/highlight paths from `editor`
  (element, inlays, inlay_map, display_map) and delete
  `edit_prediction_tests.rs`. This is the clean end state.
- **Fallback:** keep a minimal internal type for the render path in
  `editor` and drop the external crate. Only do this if 1b balloons; record
  it as debt.

The `EditPredictionKeybindAction` enum in `editor.rs:684` and the keybinding
context handling around `editor.rs:3247`–`:3275` and `:10010` belong to this
step.

### 1c — Settings, keymaps, i18n

- `crates/settings_content/src/language.rs`
  - `:41` `edit_predictions` field and `:56` merge
  - `:514` `show_edit_predictions`
  - `:546` `edit_predictions_disabled_in`
  - `:146` doc example
- `assets/settings/default.json` lines 521, 534, 1752, 2253, 2646.
- Keymaps (`assets/keymaps/default-macos.json` and the Linux equivalent):
  - `:44` `edit_prediction::ToggleMenu`
  - `:153` `editor::ToggleEditPrediction`
  - `:185`–`:196` the `edit_prediction` context bindings
  - `:908`–`:911` accept/next-word bindings
- i18n: remove the `editor.edit_prediction.*` and
  `settings_ui.edit_prediction_provider_setup.*` keys from both
  `assets/locales/en.json` and `assets/locales/zh-CN.json`. Keep the two
  catalogs in sync.

### 1d — Delete crates

1. Remove the six crates from `Cargo.toml` members and workspace
   dependencies.
2. Delete the crate directories.
3. `cargo check --workspace`; every remaining error is a missed breakpoint.

## Drag-along crates (flagged, later pass)

Removing `edit_prediction` and `web_search` removes most consumers of:

- `ollama`, `llama_cpp` (provider crates)
- `cloud_llm_client`, `cloud_api_types` (cloud model client)
- `zeta_prompt`
- parts of `copilot`, `copilot_ui`, `copilot_chat`

These belong to the provider-stack pass, not this one. `cloud_api_types` is
still used by the extension crates and `title_bar`, so it cannot be deleted
just because the model client goes. Do not bundle them into Phase 1.

The provider pass is [ACP-only Provider Stack Cleanup Plan](./acp-only-ai-provider-cleanup.md).
Sequence it after Phase 1 because Phase 1 removes the largest non-ACP model
callers.

## Removal order

1. Phase 0: `web_search`.
2. Phase 1a: unwire `zzz`, `settings_ui`, `language_tools`.
3. Phase 1b: decouple `editor`.
4. Phase 1c: settings, keymaps, i18n.
5. Phase 1d: delete crates and workspace entries.
6. Later: provider stack.

## Verification per phase

| Check                                  | Command                                |
| -------------------------------------- | -------------------------------------- |
| No dangling references                 | `rg -n "<crate>" crates Cargo.toml`    |
| Type/borrow correctness                | `cargo check --workspace`              |
| Lint                                   | `./script/clippy`                      |
| Tests                                  | `cargo test --workspace`               |
| No new hosts introduced                | `./script/check-philosophy`            |
| Docs formatting (if this file changes) | `cd docs && npx prettier --check src/` |

`cargo test --workspace` and cross-platform checks are the known weak spot in
past upstream syncs; run them here so a removal that only compiles on Linux
does not slip through.

## Repeatable procedure

To find the next dead chunk:

1. `cargo tree -i <crate> -p zzz` lists who pulls a crate in.
2. Delete the workspace dependency, then `cargo metadata` reports every
   manifest still naming it.
3. `rg -n "<symbol>"` finds registration sites that are not compile-checked
   (string namespaces, keymap contexts, action lists, settings keys).
4. Orphaned manifest entries with no code reference (like
   `crates/agent/Cargo.toml:74`) are the highest-signal finds: a dependency
   without a use means a previous removal stopped halfway.

## Out of scope

- Provider stack removal (`language_model`, `language_models`, provider
  crates, `copilot` provider) and shrinking `script/philosophy-allowlist`.
- ACP agent-side changes; OpenCode's question/elicitation bridge is tracked
  upstream (anomalyco/opencode#38121) and is not ZZZ work.
- Deciding whether ZZZ re-exposes the elicitation rail to any agent that
  emits it. The client surface already exists.
