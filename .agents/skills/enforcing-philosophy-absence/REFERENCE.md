# Enforcing Philosophy Absence Reference

Normative inventory and conventions for the
`enforcing-philosophy-absence` skill.

## Philosophy

From the root `README.md`:

> Some things should not be configurable. They should simply be absent.
>
> Strip away the telemetry, the upsells, the proprietary coupling.
> What remains is the editor.

ZZZ is local-first, no-account, ACP-only. AI stays when it points at
infrastructure the user owns. Commercial APIs are silent and manual.
No pre-built binaries yet. `script/check-philosophy` currently passes
and does not cover this remaining inventory.

## Out Of Scope

Leave these alone unless the user names them:

- Absorbing Zed upstream (`/absorbing-upstream`)
- Shipping binaries, Homebrew, or winget
- Deleting the `collab`, `copilot`, `cloud_api_*`, or
  `language_models_cloud` crates
- `Cargo.toml` git dependencies on `github.com/zed-industries/*`
- `docs/src/development/upstream-sync-*.md` historical audits
- `assets/licenses.md` third-party copyright notices
- Theme `$schema` URLs at `https://zed.dev/schema/...` (upstream schema)
- Extension Gallery traffic to `https://api.zed.dev`

## Keep

- Local Ollama (`localhost:11434`) and llama.cpp (`localhost:8080`)
- Agent `default_model.provider: "ollama"` with empty model name
- `EditPredictionProvider::None` as the enum default
- `auto_update: false` and loopback `server_url`
- `telemetry::event!` as a no-op macro until the dead trait is removed
- `CloudLanguageModelProvider` unregistered unless the user sets a
  remote `server_url`
- Help menu without Twitter / join-the-team / hosted docs
- `crates/feedback` URLs on Codeberg
- Embedded `remote_server`
- Language-server, DAP, Prettier, and Node auto-download
- Extension auto-install (HTML by default) and auto-update via `api.zed.dev`
- Copilot / Codestral / OpenAI / Anthropic as *optional* providers
- Zeta 1 / 2 / 2.1 *prompt formats* for local Ollama or OpenAI-compatible
  servers
- Local collab against `http://127.0.0.1:7331`

## Code Conventions

Read and obey `AGENTS.md`, `.rules`, and `docs/AGENTS.md`.

Rust:

- Prefer existing files. Do not create `mod.rs`.
- Full variable names. No abbreviations.
- No `unwrap()`. Propagate with `?` or log with `.log_err()`.
- Never `let _ =` on fallible operations.
- No comments unless the why is non-obvious. Do not summarize code.
- In `Entity::update` closures, use the inner `cx`. Do not re-enter
  entity updates.
- Dropped `cx.spawn` / `cx.background_spawn` tasks are cancelled.
- GPUI tests use executor timers, not `smol::Timer::after`.
- Lint with `./script/clippy`, never `cargo clippy`.
- Do not run `cargo test --workspace` unless asked.
- Do not fix unrelated baseline failures or formatting drift.

Docs (`docs/AGENTS.md`):

- Second person, present tense, practical, not promotional.
- No hedging ("simply", "just", "easily"). No superlatives.
- Keybindings use `{#kb ...}` / `{#action ...}`, never hardcoded.
- Settings: Settings Editor first, then complete JSON.
- Prettier `printWidth` 80. Format only files this session touched:
  `cd docs && npx prettier --write src/<touched.md>`
- Do not run Prettier across all of `docs/src/`.
- Do not add speculative docs for unreleased features.
- Product docs must not claim hosted downloads, auto-update-on, Zeta as
  default, Zed Pro, or data sent to ZZZ servers.
- Support links go to `https://codeberg.org/ZZZEditor/ZZZ`, not
  `zed-industries/zed` or `zed.dev/community-links`.
- Do not hotlink `https://zed.dev/img/...` or `https://images.zed.dev/...`.
  Remove the image markdown if there is no in-repo asset.

Legal and install scripts: complete English sentences. Match the tone of
`legal/terms.md`.

Locales: change `assets/locales/en.json` and `zh-CN.json` together. Keep
keys that still have callers. Rewrite values that still sell Zed AI,
Zeta-as-built-in, or Copilot subscriptions.

Tests: if a test exists only to cover rejected plan/sign-in/collection
UX, delete the test with the UX. Update tests that still apply. Do not
leave tests that require `Plan::ZedPro` chips or data-collection opt-in.

## Work Order

Complete each group before the next. Verify after each group.

### Group 1 — Product docs that still sell Zed cloud

Rewrite. Do not leave a page that contradicts `README.md`.

| File | Required change |
| --- | --- |
| `docs/src/update.md` | Auto-update is off. No hosted updater. Build from source. |
| `docs/src/reference/all-settings.md` | `auto_update` default `false`. Edit prediction provider default is omitted/`none`, not `"zed"`. Remove Zeta-as-default examples. |
| `docs/src/ai/overview.md` | Local Ollama/llama.cpp first. No Zeta default. Copilot/Codestral only as explicit opt-in. |
| `docs/src/ai/edit-prediction.md` | Title/body: local provider preference. Remove "Configuring Zeta" as hosted setup. Keep local zeta/zeta2 *format* examples. Copilot sign-in is not a getting-started path. |
| `docs/src/completions.md` | Do not call Zeta "ZZZ's own model". Point at edit-prediction docs. |
| `docs/src/ai/ai-improvement.md` | Delete the page, or replace with a short "ZZZ does not collect training data" notice. Remove it from `docs/src/SUMMARY.md`. |
| `docs/src/ai/privacy-and-security.md` | Local-first. No "we collect data to improve the product". Do not link ai-improvement as a data-sharing program. |
| `docs/src/installation.md` | Source build only. Remove brew/winget/`./script/install.sh` as hosted download. |
| `docs/src/migrate/vs-code.md` | Source build. No `zed.dev/download`. No ZZZ Pro. No "Sign in to GitHub" as the AI path. |
| `docs/src/migrate/intellij.md` | Same as vs-code. |
| `docs/src/migrate/pycharm.md` | Same as vs-code. |
| `docs/src/migrate/webstorm.md` | Same as vs-code. |
| `docs/src/migrate/rustrover.md` | Same as vs-code. |
| `docs/src/remote-development.md` | No `zed.dev/releases` download. Embedded remote_server. Remove product-page link. |
| `docs/src/worktree-trust.md` | Do not say Copilot is globally installed by default. |
| `docs/src/ai/llm-providers.md` | Copilot Chat is optional and silent. Do not lead with "Sign in to use GitHub Copilot". |
| `docs/src/ai/external-agents.md` | Claude/ChatGPT login belongs to the external agent, not ZZZ. |
| `docs/src/uninstall.md` | Community link → Codeberg. |
| `docs/src/troubleshooting.md` | Issues → Codeberg. Remove Discord/`zed.dev` staff support. Remove `images.zed.dev` screenshots. |
| `docs/src/getting-started.md` | Discussions → Codeberg issues. |
| `docs/src/linux.md` | Issue links: Codeberg, or mark upstream Zed issues as upstream. |
| `docs/src/development/feature-process.md` | Feature requests go to this repository, not `zed-industries` discussions. |
| `docs/src/development/release-notes.md` | Do not describe hosted Zed release automation as ZZZ process. |
| `docs/src/development/glossary.md` | Relative docs links, not `zed.dev/docs`. |
| `CODE_OF_CONDUCT.md` | Stop redirecting to `zed.dev/code-of-conduct`. Inline or point at this repo. |
| `extensions/README.md` | Relative repo paths. Marketplace may mention `api.zed.dev` as user-initiated. |

Keep as-is if still accurate: `docs/src/ai/billing.md`,
`subscription.md`, `plans-and-usage.md`, `models.md`,
`configuration.md`, `docs/src/development/privacy-boundary.md`,
`legal/terms.md`.

### Group 2 — Legal

Rewrite to match `legal/terms.md`. ZZZ has no accounts, payments,
telemetry, hosted AI, or subprocessors by default.

| File | Required change |
| --- | --- |
| `legal/privacy-policy.md` | Full rewrite. No Zed Industries, `privacy@zed.dev`, opt-out telemetry, or subscription data. |
| `legal/subprocessors.md` | Replace with "no default subprocessors" or delete and drop dead links. |
| `legal/third-party-terms.md` | Only terms of providers the user configures. No "Zed hosts on your behalf". |

### Group 3 — Install and release scripts

| File | Required change |
| --- | --- |
| `script/install.sh` | Must not curl `cloud.zed.dev` or `zed.dev/releases`. Print that ZZZ has no hosted binaries and point at local build docs. |
| `script/lib/deploy-helpers.sh` | Do not print `collab.zed.dev` as a ZZZ destination. |
| `tooling/xtask/src/tasks/workflows/after_release.rs` | Do not POST `cloud.zed.dev/releases/refresh` or open `zed.dev/releases`. |

### Group 4 — Live product code

Delete the UX, not the whole crate. If a compile error needs a rejected
type, revert and cut a smaller surface.

**Plans and onboarding**

- `crates/ai_onboarding/src/plan_definitions.rs`: remove plan/trial/upsell
  lists. If the crate still needs a component, render provider-neutral
  "configure a local provider" copy only.
- `crates/ai_onboarding/src/young_account_banner.rs`: delete the banner
  and all call sites.
- `crates/ai_onboarding/src/ai_onboarding.rs`: remove `Plan::Zed*`
  branches, `sign_in` CTA, "Welcome to Zed AI". Empty `sign_in`
  callbacks are not enough.
- `crates/ai_onboarding/src/agent_panel_onboarding_content.rs`: remove
  `Plan::ZedProTrial` / `ZedPro` checks.
- `crates/agent_ui/src/agent_configuration.rs`: delete
  `render_zed_plan_info` and its call sites (Free/Pro/Trial/Business/
  Student chips).
- `crates/onboarding/src/onboarding.rs`: rename or remove the no-op
  `SignIn` action so the public action is not a sign-in.

Do not keep `Plan` chips with neutralized copy. Absence means the chip
is gone.

**Collab**

- Multi-user collab crates are absent: `call`, `channel`, `collab`,
  `collab_ui`, `livekit_client`, `livekit_api`.
- Keep SSH/WSL/Docker remote development (`Project::remote`).

**Copilot**

- Keep `crates/copilot`, `copilot_ui`, `copilot_chat`.
- Remove subscription / "Sign in to use GitHub Copilot" as a default
  path. Status-bar entry may exist only after the user selects Copilot
  as provider.
- Rewrite remaining copy in `crates/copilot_ui/src/sign_in.rs` so it
  does not pitch GitHub subscriptions.

**Zeta hosted collection**

- `crates/edit_prediction/src/edit_prediction.rs`: `can_collect_data`
  must be false. Remove `rand::random_ratio(1, 1000)` capture.
- `crates/edit_prediction/src/zeta.rs`: do not attach repo URLs or
  capture payloads for training.
- `crates/edit_prediction/src/zed_edit_prediction_delegate.rs`: remove
  data-collection toggle API usage from the product UI path.
- `crates/edit_prediction_ui/src/edit_prediction_button.rs`: remove
  "Training Data Collection" menu and "Powered by Zeta".
- `crates/edit_prediction_ui`: rating modal and upload entry point
  are absent. Do not restore `rate_prediction_modal.rs`.
- `crates/settings_content/src/language.rs`:
  `EditPredictionProvider::Zed` must not display as "Zed AI".
  `allow_data_collection` default is never collect.
- `crates/settings_ui/src/page_data.rs`: remove "built-in Zeta model"
  copy and the data-collection control, or make the control absent.

**Hosted URLs in code**

- `crates/context_server/src/oauth.rs`: `CIMD_URL` must not be
  `https://zed.dev/oauth/client-metadata.json`. Use a loopback or
  omit client-metadata URL if the MCP OAuth flow allows. Do not invent
  a hosted ZZZ OAuth service.
- `crates/cloud_api_client/src/cloud_api_client.rs`: do not default the
  host to `cloud.zed.dev`. Use the configured `server_url` only.
- `crates/http_client/src/http_client.rs`: keep
  `build_zed_extension_marketplace_url` → `api.zed.dev`. Cloud/LLM URL
  builders must not map an unset local `server_url` onto
  `cloud.zed.dev`.
- `crates/client/src/zed_urls.rs`: keep `about:blank`; fix comments that
  still say "Zed AI".

### Group 5 — Defaults and locales

`assets/settings/default.json`:

- Delete the `edit_predictions.copilot` object from defaults.
- Delete the `edit_predictions.codestral` object (including
  `https://codestral.mistral.ai`) from defaults.
- Set `edit_predictions.allow_data_collection` to `"no"`, or remove the
  key if the schema default is never-collect.
- Delete `language_models["zed.dev"]`.
- Keep other provider `api_url` values; they are contacted only after
  the user selects that provider.
- Do not set `edit_predictions.provider` to `"zed"` or `"copilot"`.

`assets/settings/initial_*.json`: replace `https://zed.dev/docs/...`
comments with in-repo doc paths.

`assets/locales/en.json` and `zh-CN.json`:

- `ai_onboarding.welcome_zed_ai`
- `settings_ui.page_data.description.set.up.different.edit.prediction.providers.in.complement.to.zed.s.built.in.zeta.model`
- `edit_prediction_ui.button.powered_by_zeta`
- `copilot_ui.sign_in.*` subscription / "Sign in to use GitHub Copilot"
  strings

### Group 6 — Hosted images and leftover zed.dev links

Remove or replace. Do not leave broken `![...](https://zed.dev/...)`.

Files known to hotlink product assets or site pages:

- `docs/src/project-panel.md`
- `docs/src/troubleshooting.md`
- `docs/src/outline-panel.md`
- `docs/src/command-palette.md`
- `docs/src/tab-switcher.md`
- `docs/src/repl.md`
- `docs/src/remote-development.md`
- `docs/src/themes.md`
- `docs/src/icon-themes.md`
- `docs/src/extensions/themes.md`
- `docs/src/extensions/icon-themes.md`
- `docs/src/extensions/mcp-extensions.md`
- `docs/src/extensions/debugger-extensions.md`
- `docs/src/languages/{toml,java,ocaml,python,proto,ansible,diff}.md`
- `docs/src/tasks.md`
- `docs/src/globs.md`

`https://zed.dev/schema/...` may stay. `https://api.zed.dev` may stay
when documenting user-initiated gallery traffic.

### Group 7 — Dead telemetry APIs

`telemetry::event!` is already a no-op. `Item::telemetry_event_text`
still returns event names and has no live sender.

This group is second-knife. After groups 1–6:

- Return `None` from remaining `telemetry_event_text` impls, or remove
  the trait method and all impls if that compiles without new APIs.
- Known `Some("... Opened")` impls include onboarding, welcome,
  audio_viewer, extensions_ui, project search, REPL, diagnostics,
  git_ui diffs/commits, markdown preview, language_tools views,
  pdf/svg viewers, dap_log, agent_diff, agent_registry.
- `crates/action_log` empty `telemetry_report_*` shims and
  `crates/language_models/src/provider/anthropic/telemetry.rs` may stay
  as no-ops until a later crate deletion.

Do not reintroduce a telemetry crate.

### Group 8 — Extend `script/check-philosophy`

Add checks that would have caught this pass. Keep the script in bash +
`rg` like the existing checks.

Required new assertions:

- `docs/src/update.md` does not say auto-update is on by default.
- `docs/src/reference/all-settings.md` does not document `auto_update`
  default `true` or edit prediction provider default `"zed"`.
- `docs/src/ai/overview.md` / `completions.md` / `edit-prediction.md`
  do not call Zeta the default provider.
- `script/install.sh` does not contain `cloud.zed.dev` or
  `zed.dev/releases`.
- `legal/privacy-policy.md` does not contain `privacy@zed.dev`,
  `Zed Industries`, or "opt out of Software telemetry".
- `assets/settings/default.json` has no `language_models` key `zed.dev`,
  no `edit_predictions.codestral.api_url` on `codestral.mistral.ai`,
  and `allow_data_collection` is not `"yes"`.
- `crates/context_server/src/oauth.rs` does not hardcode
  `https://zed.dev/oauth/client-metadata.json`.
- `crates/ai_onboarding` and `crates/agent_ui` have no
  `render_zed_plan_info` / `PlanDefinitions.pro_plan` product chips.
- Multi-user collab crates (`call`, `channel`, `collab`, `collab_ui`,
  `livekit_client`, `livekit_api`) are absent.

Do not weaken existing checks.

## Verification

After each group, run the narrowest commands that cover touched files.

```sh
./script/check-philosophy
git diff --check
```

Rust, only touched crates:

```sh
cargo check --locked -p <crate>
cargo test --locked -p <crate> <test_filter>
```

Typical crates for group 4: `zzz`, `ai_onboarding`, `agent_ui`,
`edit_prediction`, `edit_prediction_ui`, `settings_ui`,
`settings_content`, `context_server`, `cloud_api_client`, `http_client`,
`client`, `language_models`.

Docs, only touched files:

```sh
cd docs && npx prettier --write src/<file.md>
cd docs && npx prettier --check src/<file.md>
```

Do not run `cargo test --workspace`, `cargo fmt --all`, or Prettier on
all of `docs/src/`. Record macOS/Windows runtime as `NOT RUN`.

If Prettier or rustfmt rewrites an unrelated region, restore it.

## Hard Stops

Stop and leave the tree clean if:

- a deletion cannot compile without importing account, billing,
  telemetry, native-agent, or hosted-Zed APIs
- `script/install.sh` would still download a binary after the edit
- a doc rewrite would claim ZZZ ships Zeta, auto-update, or accounts
- the user asked only for a subset and the next group is out of that
  subset

Record the file, the reason, and which groups already passed.

## Commits

Do not commit unless the user explicitly asks. If they do:

- `git commit -s`
- English imperative subject
- No CLA
- Stage only files from this pass
- Do not skip hooks or force-push
