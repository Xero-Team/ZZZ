---
name: enforcing-philosophy-absence
description:
  "Guards ZZZ's local-first absence boundary after the commercial, account,
  telemetry, hosted-docs, and default-network surfaces were removed. Use when
  a change reintroduces Zed cloud copy, hosted binaries, Zeta/Copilot
  defaults, PlanDefinitions, CIMD_URL, allow_data_collection, or collab
  sign-in, when extending script/check-philosophy with a new regression
  assertion, or when verifying cargo xtask check philosophy. Do not use for
  absorbing upstream."
license: CC-BY-4.0
compatibility: opencode
metadata:
  prompt-language: English
  user-language: Simplified Chinese
---

Follow `REFERENCE.md` in this skill first. The absence pass is complete:
the rejected crates and UX are gone, and `script/check-philosophy` now
carries 76 regression assertions that pass on a clean tree. If philosophy
and a clean compile conflict, delete the surface; never import rejected
account, billing, telemetry, or hosted-Zed machinery to keep the trees
matching.

# Enforcing Philosophy Absence

You guard the local-first absence boundary. A future change (an upstream
sync, a docs rewrite, a new setting) must not reintroduce Zed cloud,
accounts, telemetry, hosted downloads, or a default network call. The
original removal work is recorded in `REFERENCE.md` as a completed
inventory and a set of invariants, not as a to-do list.

## Communication Rules

- Keep this skill prompt written in English.
- Interact with the user in Simplified Chinese.
- Write code comments (only when required), commit messages, legal text,
  and documentation in complete English. Do not compress legal or docs.
- When a technical term could be ambiguous, add a short Chinese gloss
  followed by the English term in parentheses.

## When to Use

- A diff or upstream absorption reintroduces a rejected surface: Zed Pro,
  Zeta-as-default, auto-update-on copy, `privacy@zed.dev`,
  `install.sh` hosted downloads, `PlanDefinitions`, `CIMD_URL` pointing at
  `zed.dev`, `allow_data_collection`, or collab sign-in.
- The user asks to extend `script/check-philosophy` with a new regression
  assertion, or to verify `cargo xtask check philosophy` after a change.
- A new setting, feature flag, or default would re-enable a surface that
  must stay absent.

Do not use this skill for absorbing Zed `main` (use
`absorbing-upstream`), shipping binaries, or ordinary editor features.

## Operating Rules

- Absence over configuration. Do not add a setting, feature flag, or
  fallback that re-enables a rejected surface; delete the surface.
- Keep AI that is provider-agnostic and local-first. Ollama and
  llama.cpp stay preferred. Silent manual commercial APIs stay.
- The collab, copilot, cloud_api_client, ai_onboarding, edit_prediction,
  and edit_prediction_ui crates are already absent. Never recreate them
  or their plan/sign-in/collection UX.
- `assets/settings/default.json` has no `zed.dev` language model,
  `edit_predictions.codestral`, or `allow_data_collection`; keep it that
  way. `script/install.sh` has no `cloud.zed.dev` / `zed.dev/releases`
  download. `crates/context_server/src/oauth.rs` `CIMD_URL` is loopback.
- `telemetry::event!` and `Item::telemetry_event_text` are gone. Do not
  reintroduce a telemetry crate, trait method, or event sender.
- `script/check-philosophy` is the regression net: it has 76 assertions
  and is invoked by both `cargo xtask check` (`check all`) and the
  `cargo xtask clippy` preflight. Do not weaken a check to make a change
  pass; fix the change or add a targeted assertion.
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
2. Confirm a clean worktree except files this session owns. Run
   `cargo xtask check philosophy` to establish the baseline passes.
3. Identify the surface the change touches. Use the completed inventory
   in `REFERENCE.md` to decide whether it is a regression, a new gap, or
   an allowed local-only feature.
4. Fix by deleting the reintroduced surface, not by gating it. Keep the
   smallest diff. If `cargo check` fails, revert and cut a smaller surface;
   do not add unused types so a deletion compiles.
5. If a regression slipped through, add a `check` assertion to
   `script/check-philosophy` following its existing bash + `rg` style,
   then re-run the gate. Do not weaken existing checks.
6. Format only touched docs with Prettier (`printWidth` 80) and only
   touched Rust with `cargo fmt` on those crates; do not rewrite unrelated
   drift. Re-run `cargo xtask check philosophy` and `git diff --check`.
7. Stop after the requested scope.

## Final Response

Summarize in Simplified Chinese:

- whether the boundary was preserved, restored, or newly asserted
- files changed and any `script/check-philosophy` assertion added
- `cargo xtask check philosophy` PASS/FAIL and the narrow `cargo check`
  results for touched crates
- anything left because a hard stop fired
