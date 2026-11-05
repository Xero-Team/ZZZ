# Enforcing Philosophy Absence Reference

Normative inventory and conventions for the
`enforcing-philosophy-absence` skill.

The absence pass is complete. This reference records the resulting
invariants and the regression net that protects them. It is not a to-do
list; a future change must not reintroduce a removed surface.

## Philosophy

From the root `README.md`:

> Some things should not be configurable. They should simply be absent.
>
> Strip away the telemetry, the upsells, the proprietary coupling.
> What remains is the editor.

ZZZ is local-first, no-account, ACP-only. AI stays when it points at
infrastructure the user owns. Commercial APIs are silent and manual.
There are no pre-built hosted binaries.

`script/check-philosophy` is the regression net: 67+ assertions in bash +
`rg`, invoked by `cargo xtask check` (`check all`) and by the
`cargo xtask clippy` preflight. It passes on a clean `main`. Keep new
checks in the same style and never weaken an existing assertion.

## Out Of Scope

Leave these alone unless the user names them:

- Absorbing Zed upstream (`/absorbing-upstream`)
- Shipping binaries, Homebrew, or winget
- `Cargo.toml` git dependencies on `github.com/zed-industries/*`
- `docs/src/development/upstream/upstream-sync-*.md` historical audits
- `assets/licenses.md` third-party copyright notices
- Theme `$schema` URLs at `https://zed.dev/schema/...` (upstream schema)
- User-initiated Extension Gallery traffic to `https://api.zed.dev`

The crates `collab`, `call`, `channel`, `collab_ui`, `livekit_client`,
`livekit_api`, `copilot`, `copilot_ui`, `copilot_chat`,
`cloud_api_client`, `language_models_cloud`, `ai_onboarding`, and
`edit_prediction_ui` are already deleted. `crates/cloud_api_types`
remains as a plain type crate. Recreating any deleted crate or its
plan/sign-in/collection UX is a regression, not a design choice.

## Keep (verified present)

- Local Ollama (`localhost:11434`) and llama.cpp (`localhost:8080`)
- Agent `default_model.provider: "ollama"` with empty model name
- `EditPredictionProvider::None` as the enum default
- `auto_update: false` and loopback `server_url`
- Help menu without Twitter / join-the-team / hosted docs
- `crates/feedback` URLs on GitHub
- Embedded `remote_server`
- Language-server, DAP, Prettier, and Node auto-download
- Extension auto-install (HTML by default) and user-initiated gallery
  traffic via `api.zed.dev`
- Zeta 1 / 2 / 2.1 *prompt formats* for local Ollama or
  OpenAI-compatible servers
- Local collab against `http://127.0.0.1:7331`
- Provider-agnostic, silent manual AI providers (`language_model` and
  friends) with no subscription or sign-in default

`Item::telemetry_event_text`, `telemetry::event!` as a live macro, the
hosted `EditPredictionProvider::Zed` default, plan chips, and the
"Training Data Collection" menu are gone; do not restore them.

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
- Lint with `cargo xtask clippy`, never `cargo clippy`.
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
- Support links go to `https://github.com/Xero-Team/ZZZ`, not
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

## Completed Boundary (regression triggers)

Each item below is done and covered by `script/check-philosophy`. Treat a
reappearance as a regression: delete the surface, then add a check if the
gate missed it.

### Product docs

`docs/src/update.md`, `reference/all-settings.md`, `ai/overview.md`,
`ai/edit-prediction.md`, `completions.md`, `installation.md`, the
`migrate/*` pages, `remote-development.md`, `worktree-trust.md`,
`ai/llm-providers.md`, `ai/external-agents.md`, `uninstall.md`,
`troubleshooting.md`, `getting-started.md`, `linux.md`,
`development/feature-process.md`, `development/release-notes.md`,
`development/glossary.md`, `CODE_OF_CONDUCT.md`, and
`extensions/README.md` no longer sell Zed cloud, hosted downloads,
Zeta-as-default, Zed Pro, Copilot sign-in as a default path, Discord/
`zed.dev` support, or `images.zed.dev` screenshots. `ai/ai-improvement.md`,
`ai/billing.md`, `ai/subscription.md`, `ai/plans-and-usage.md` are absent.
Support and feature-request links point at this repository.

### Legal

`legal/privacy-policy.md` states ZZZ has no accounts, telemetry, payments,
or hosted AI. `legal/subprocessors.md` says "no default subprocessors".
`legal/third-party-terms.md` covers only providers the user configures.
No `privacy@zed.dev`, "Zed Industries", or telemetry opt-out copy remains.

### Install and release scripts

`script/install.sh` has no `cloud.zed.dev` / `zed.dev/releases` download
and points at local builds. `script/lib/deploy-helpers.sh` does not print
`collab.zed.dev`. The hosted `after_release.rs` workflow generator and
its `cloud.zed.dev/releases/refresh` POST are gone.

### Live product code

- The `ai_onboarding` crate, its `PlanDefinitions` / young-account banner,
  and `render_zed_plan_info` plan chips are absent. `crates/onboarding`
  has no public `SignIn` action.
- Multi-user collab crates (`call`, `channel`, `collab`, `collab_ui`,
  `livekit_client`, `livekit_api`) are absent; SSH/WSL/Docker remote
  development (`Project::remote`) stays.
- Copilot crates and their subscription sign-in UX are absent. Provider-
  agnostic AI remains with no default sign-in path.
- The `edit_prediction` and `edit_prediction_ui` crates are absent, so
  the "Training Data Collection" menu, `rate_prediction_modal.rs`, Zeta
  hosted collection capture, `can_collect_data`, and the
  `allow_data_collection` default are gone with them.
- `crates/context_server/src/oauth.rs` `CIMD_URL` is
  `http://127.0.0.1/oauth/client-metadata.json`, not a hosted
  `zed.dev` URL. Cloud/LLM URL builders do not fall back to
  `cloud.zed.dev` when `server_url` is unset. `api.zed.dev` remains only
  for user-initiated gallery traffic.

### Defaults and locales

`assets/settings/default.json` has no `language_models["zed.dev"]`, no
`edit_predictions.codestral.api_url` on `codestral.mistral.ai`, no
`allow_data_collection`, and no `edit_predictions.provider` set to
`"zed"` or `"copilot"`. `initial_*.json` comments use in-repo doc paths.
The commercial, Zeta-built-in, and Copilot-subscription locale keys were
removed from both catalogs; the two catalogs keep identical key sets
(`cargo xtask check locale-keys`).

### Hosted links and telemetry

No `![...](https://zed.dev/...)` or `https://images.zed.dev/...` hotlinks
remain in `docs/src/`. `https://zed.dev/schema/...` may stay;
`https://api.zed.dev` may stay for user-initiated gallery traffic.
`telemetry::event!` is not a live sender, `Item::telemetry_event_text` is
absent, and no telemetry crate or directory was reintroduced.

### Regression net

`script/check-philosophy` asserts, among 67+ checks: auto-update is off,
edit prediction does not default to `"zed"` or Copilot, Zeta is not called
the default provider, `install.sh` has no hosted binaries, the privacy
policy has no Zed Industries telemetry opt-out, `default.json` has no
hosted AI collection, `CIMD_URL` is not hosted, plan chips and
`render_zed_plan_info` are absent, multi-user collab crates are absent,
`cloud_api_client` is absent, `Item::telemetry_event_text` is absent, and
network hosts are allowlisted. Read the script's `check "..."` lines for
the authoritative list before adding a new one.

## Verification

After any change that touches a guarded surface, run the narrowest
commands that cover the touched files.

```sh
cargo xtask check philosophy
git diff --check
```

Rust, only touched crates:

```sh
cargo check --locked -p <crate>
cargo test --locked -p <crate> <test_filter>
```

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

- a fix cannot compile without importing account, billing, telemetry,
  native-agent, or hosted-Zed APIs
- `script/install.sh` would still download a binary after the edit
- a doc rewrite would claim ZZZ ships Zeta, auto-update, or accounts
- the user asked only for a subset and the next change is out of that
  subset

Record the file, the reason, and which invariants already passed.

## Commits

Do not commit unless the user explicitly asks. If they do:

- `git commit -s`
- English imperative subject
- No CLA
- Stage only files from this pass
- Do not skip hooks or force-push
