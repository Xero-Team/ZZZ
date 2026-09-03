---
name: enforcing-philosophy-absence
description:
  "Makes remaining ZZZ commercial, account, telemetry, hosted-docs, and
  default-network surfaces absent. Use when rewriting product docs that
  still sell Zed cloud, legal/privacy-policy, install.sh cloud.zed.dev
  downloads, Zeta/Copilot defaults, PlanDefinitions, CIMD_URL,
  allow_data_collection, collab GitHub sign-in, or extending
  script/check-philosophy. Do not use for absorbing upstream."
license: CC-BY-4.0
compatibility: opencode
metadata:
  prompt-language: English
  user-language: Simplified Chinese
---

Follow `REFERENCE.md` in this skill first. If philosophy and a clean
compile conflict, delete the surface; never import rejected account,
billing, telemetry, or hosted-Zed machinery to keep the trees matching.

# Enforcing Philosophy Absence

You finish the local-first absence pass. `script/check-philosophy` already
passes. Remaining work is product copy, legal, install scripts, live
upsell/account/data-collection UX, and defaults that still describe Zed
cloud. Things that should not be configurable must be absent.

## Communication Rules

- Keep this skill prompt written in English.
- Interact with the user in Simplified Chinese.
- Write code comments (only when required), commit messages, legal text,
  and documentation in complete English. Do not compress legal or docs.
- When a technical term could be ambiguous, add a short Chinese gloss
  followed by the English term in parentheses.

## When to Use

- The user asks to apply the remaining philosophy pass, strip hosted
  coupling, or execute the absence inventory.
- The user names leftover Zed Pro, Zeta-as-default, auto-update docs,
  privacy-policy, `install.sh`, `PlanDefinitions`, or `CIMD_URL`.

Do not use this skill for absorbing Zed `main`, shipping binaries, or
ordinary editor features.

## Operating Rules

- Absence over configuration. Do not add a setting that disables a
  rejected surface; delete the surface.
- Keep AI that is provider-agnostic and local-first. Ollama and
  llama.cpp stay preferred. Silent manual commercial APIs stay.
- Keep Copilot and Codestral as optional providers. Remove them from
  defaults and from first-run / status-bar upsell.
- Keep Zeta *prompt formats* for local models. Remove hosted
  `EditPredictionProvider::Zed` from product defaults and collection UX.
- Multi-user collab, LiveKit, and channel chat are absent (Plan B).
- Do not delete the `copilot`, `cloud_api_*`, or
  `language_models_cloud` crates in this pass.
- Do not absorb upstream, add remotes, push, open a PR, or commit unless
  the user explicitly asks.
- Do not download or hotlink replacement screenshots from `zed.dev`.
- Prefer existing files. No `mod.rs`. No `unwrap()`. No `let _ =` on
  fallible ops. No comments except a non-obvious why. Full variable names.
- In `Entity::update` closures, use the inner `cx`.
- Match surrounding style. Do not reformat unrelated regions.

## Procedure

1. Read `README.md`, `AGENTS.md`, `docs/AGENTS.md`, `.rules`,
   `script/check-philosophy`, and this skill's `REFERENCE.md`.
2. Confirm a clean worktree except files this session owns.
3. Apply `REFERENCE.md` in order: docs, legal, install scripts, live
   code, defaults/locales, hosted links, dead telemetry APIs,
   `script/check-philosophy`.
4. After each group, run the narrowest verification in `REFERENCE.md`.
   If `cargo check` fails, revert that group and narrow the deletion.
   Do not add unused types so a later deletion compiles.
5. Format only touched docs with Prettier (`printWidth` 80). Format only
   touched Rust with `cargo fmt` on those crates if the crate is already
   clean; do not rewrite unrelated formatting drift.
6. Stop after the requested groups, or after the full pass if the user
   did not limit scope.

## Final Response

Summarize in Simplified Chinese:

- groups completed
- files changed
- checks run and their PASS/FAIL
- anything left because a hard stop fired
