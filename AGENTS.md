# ZZZ agent notes

## Workspace
- Root is a Rust workspace; default member is `crates/zed`.
- Main app entrypoint lives in `crates/zed`; common touchpoints are `crates/gpui`, `crates/editor`, `crates/project`, `crates/workspace`, `crates/vim`, `crates/lsp`, `crates/rpc`, and `crates/ui`.
- Extensions live under `extensions/`; rebuild generated workflow files with `cargo xtask workflows`.
- Docs live in `docs/` and have their own `docs/AGENTS.md`.

## Commands
- Run app: `cargo run`
- Fast dev profile: `cargo run --profile release-fast`
- Full workspace tests: `cargo test --workspace`
- Lint: `./script/clippy` (not `cargo clippy`)
- Local backend bootstrap: `script/bootstrap`, then `foreman start`, then `script/zed-local -2|-3|-4`
- Linux install/build quirk: `REMOTE_SERVER_TARGET=x86_64-unknown-linux-gnu script/install-linux`
- Docs preprocessor: `cargo run -p docs_preprocessor --` and `cargo run -p docs_preprocessor -- postprocess`
- Docs formatting: `cd docs && npx prettier --write src/`; verify with `cd docs && npx prettier --check src/`

## Rust / GPUI
- Avoid `unwrap()` and `let _ =` on fallible ops; propagate or log errors.
- Prefer existing files; avoid `mod.rs`; use full variable names.
- In `Entity::update` closures, use inner `cx`; do not re-enter entity updates.
- Dropped `cx.spawn` / `cx.background_spawn` tasks are cancelled; await, detach, or store them.
- In GPUI tests, use executor timers, not `smol::Timer::after(...)`.
- `clippy.toml` also disallows `std::process::Command::*` and `smol::Timer::after`; use `smol::process::Command` and GPUI timers.

## Contributions
- README says DCO/no CLA; use `git commit -s`. `CONTRIBUTING.md` is upstream-stale on this point.
- If you discover a reusable pattern, add it to `.rules` only after validation; include a `Suggested .rules additions` section in PR text.
