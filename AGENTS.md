# ZZZ agent notes

## Workspace
- Root is a Rust workspace; default member is `crates/zzz`.
- Main app entrypoint lives in `crates/zzz`; common touchpoints are `crates/gpui`, `crates/editor`, `crates/project`, `crates/workspace`, `crates/vim`, `crates/lsp`, `crates/rpc`, and `crates/ui`.
- Extensions live under `extensions/`; `cargo xtask workflows` regenerates the workflow files from Rust sources. The generated files are not tracked (only `ci.yaml` is), so rebuild them on demand rather than committing them.
- Docs live in `docs/` and have their own `docs/AGENTS.md`.
- Project-local OpenCode skills live under `.agents/skills/` (`creating-skills`); project-local agents belong under `.opencode/agents/` (`creating-agents`). `CLAUDE.md` and `GEMINI.md` are symlinks to `.rules`; edit `.rules`, not those symlinks.
- The pinned toolchain is Rust 1.99.1 (`rust-toolchain.toml`), mirrored by `clippy.toml` `msrv` and `Cargo.toml` `[workspace.package] rust-version`. CI checks core crates against both MSRV and stable.

## Commands
- Run app: `cargo run`
- Fast dev profile: `cargo run --profile release-fast`
- Full workspace tests: `cargo test --workspace`
- Lint: `cargo xtask clippy` (not `cargo clippy`); it runs the philosophy preflight, the workspace/manifest clippy passes, the `future_not_send` pass, and local-only auxiliary checks (`typos`, `cargo-machete`, `buf`). `script/clippy` is a forwarding shim to this gate.
- Repository gates without a compiler: `cargo xtask check` (defaults to `check all`). CI runs it, plus `cargo fmt --all -- --check` and a separate MSRV/stable `cargo check` on core crates.
- `check all` enforces these gates in Rust (`tooling/xtask/src/tasks/check.rs`): todos, keymaps, scroll handlers, crate licenses, locale catalog keys, DCO sign-off, philosophy, upstream rejection ledger, and workflow files. The policy engines `script/check-philosophy` and `script/check-upstream-ledger` stay as scripts; `cargo xtask check` invokes them.
- `check links` (needs `lychee`) and `check vendor` are local-optional and are
  not part of `check all`; run them individually. `check all` also formats the
  docs and runs ShellCheck, so a local run needs Node (Prettier) and
  `shellcheck`; `check keys`, `check locale-keys`, `check dco`, `check todos`,
  `check licenses`, `check philosophy`, `check upstream-ledger`,
  `check workflows`, `check scroll-handlers`, `check docs`, and
  `check shellcheck` are also available on their own.
- Release artifacts: `cargo xtask build` (containerized by default) and `cargo xtask dist --all`; check a host with `cargo xtask doctor`
- Linux install/build quirk: `cargo xtask install local` builds and installs a
  local tarball; `REMOTE_SERVER_TARGET=x86_64-unknown-linux-gnu` still selects
  the embedded remote-server target when needed.
- Docs preprocessor: `cargo run -p docs_preprocessor --` and `cargo run -p docs_preprocessor -- postprocess`
- Docs formatting: `cd docs && npx prettier --write src/`; verify with `cd docs && npx prettier --check src/`. For a single touched file, pass `src/<path>.md` instead of `src/`.

## Rust / GPUI
- Do not add `unwrap()`/`expect()` on production paths; propagate with `?` or log errors. Tests, proven invariants, and `const` construction are exempt. Do not silently discard fallible results with `let _ =`; handle or log them. Existing call sites are not rewritten in bulk.
- Prefer existing files; avoid `mod.rs`; use full variable names.
- In `Entity::update` closures, use inner `cx`; do not re-enter entity updates.
- Dropped `cx.spawn` / `cx.background_spawn` tasks are cancelled; await, detach, or store them.
- In GPUI tests, use executor timers, not `smol::Timer::after(...)`.
- `clippy.toml` also disallows `std::process::Command::*` and `smol::Timer::after`; use `smol::process::Command` and GPUI timers.

## i18n
- Localize user-facing text with `i18n::tr(cx, key, fallback)` (some crates alias it, e.g. `app_i18n::tr`).
- Add every key to both `assets/locales/en.json` and `assets/locales/zh-CN.json`; the two catalogs must keep identical key sets.
- The catalogs are authoritative for what is displayed. Keep the in-code `fallback` identical to the `en.json` value so it does not drift (a changed fallback alone is invisible at runtime).
- Do not localize brand names, shell commands, HTTP headers, or internal/log identifiers.

## Contributions
- README and CONTRIBUTING.md both state DCO/no CLA; use `git commit -s`. `cargo xtask check dco` enforces sign-off on recent non-merge commits.
- `cargo xtask check locale-keys` enforces identical key sets in `assets/locales/en.json` and `assets/locales/zh-CN.json`; `cargo xtask check philosophy` keeps removed commercial/network surfaces absent. Do not weaken these gates.
- If you discover a reusable pattern, add it to `.rules` only after validation; include a `Suggested .rules additions` section in PR text.
