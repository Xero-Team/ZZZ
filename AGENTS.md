# ZZZ agent notes

## Workspace
- Rust workspace; `default-members = ["crates/zzz"]`, so a bare `cargo build` or `cargo test` covers only the app crate. Add `--workspace` or `-p <crate>`.
- `crates/zzz` is the app and the only crate whose version tracks upstream Zed; `[workspace.package] publish = false` everywhere, so versions exist to satisfy cargo, not a compatibility window.
- Crate kinds, and where a rule about one belongs: `docs/src/development/crates.md`. Crates that shadow a Zed crate (`gpui`, `editor`, `project`, `workspace`, `ui`, `vim`) get overwritten by the next sync, so a local edit there is a recorded divergence, not a free fix. `telegram`/`telegram_ui` are local-original.
- Upstream moves by selective cherry-pick, never by merge or fast-forward: `absorbing-upstream` skill plus `docs/src/development/upstream-cherrypick.md`. Rejected surfaces are policy data (`script/check-philosophy`, `docs/src/development/upstream-rejected.tsv`) — do not weaken them.
- Toolchain is pinned to 1.100.0 in four places that `check msrv` compares: `rust-toolchain.toml` `channel`, `clippy.toml` `msrv`, `Cargo.toml` `rust-version`, and the `toolchain:` line in `.github/workflows/ci.yaml`. Bump all four together. Installed targets cover `wasm32-wasip2` (extensions), `wasm32-unknown-unknown` (gpui web), and musl (embedded remote server).
- Linux build deps: `./script/linux`, which CI runs before every Rust job.

## Commands
- Run app: `cargo run`; profiling build: `cargo run --profile release-fast`.
- Tests: CI runs `cargo nextest run --workspace --no-fail-fast`; `cargo test --workspace` is the quick local pass. One test: `cargo test -p editor test_name`. `.config/nextest.toml` gives every test a 60s slow timeout — extend it there instead of adding sleeps.
- Nightly tool gates: `cargo xtask miri` interprets the curated crate list (PR-blocking) and `cargo xtask sanitize` builds it with AddressSanitizer (scheduled, not blocking); `--list` prints either list with reasons, `-p <crate>` runs one crate, and `cargo xtask check nightly-tools` validates both lists plus the `.cargo/config.toml` MIRIFLAGS defaults they rely on.
- Lint: `cargo xtask clippy` (not `cargo clippy`); `script/clippy` only forwards to it. It runs the philosophy preflight, the workspace and manifest passes, a curated `future_not_send` pass, then `typos`, `cargo-machete`, and `buf`.
- Compiler-free gates: `cargo xtask check`, which defaults to `check all`. `GATES` in `tooling/xtask/src/tasks/check.rs` is the list; add a gate there, not in a new script. Individual gates: `todos`, `keys`, `scroll-handlers`, `licenses`, `module-layout`, `file-size`, `msrv`, `release`, `test-markers`, `unsafe-surface`, `lints-inheritance`, `lint-suppressions`, `clippy-copies`, `locale-catalog`, `locale-keys`, `locale-fallbacks`, `locale-placeholders`, `nightly-tools`, `dco`, `shellcheck`, `docs`, `vendor`, `philosophy`, `upstream-ledger`, `workflows`, `ci-wiring`, `gpui-api`. `check all` needs Node (Prettier) and `shellcheck`; `vendor` and `links` (needs `lychee`) stay out of it.
- Generated tree-sitter sources: `cargo xtask grammars check`.
- Docs preprocessor: `cargo run -p docs_preprocessor --`, then `-- postprocess`. Docs and settings formatting: `script/prettier` or `script/prettier --write`.
- Artifacts: `cargo xtask build` (containerized by default), `cargo xtask dist --all`, `cargo xtask doctor` for host readiness, `cargo xtask install local` for a local Linux tarball.

## Performance work
- Two local-only harnesses; neither runs in CI, so a regression stays invisible until someone runs it:
  - `cargo perf-test -p <crate>` runs `#[perf]`-annotated tests under `release-fast`. Used by `gpui`, `search`, `terminal`, `util`, `vim`; harness in `tooling/perf`, macro in `crates/util_macros`.
  - `cargo bench -p <crate>` runs criterion benches (`harness = false`) in `editor`, `language`, `rope`, `fuzzy_nucleo`, `project_panel`, `extension_host`. `crates/editor/benches/editor_frame_workload.rs` measures frame time, RSS, and allocation count end to end; `crates/*_benchmarks` are standalone driver binaries, not criterion.
  - `cargo perf-compare` diffs two runs and needs `hyperfine`. It gates each metric on the A/A noise floor recorded by `cargo perf-compare noise` and refuses to diff runs from different build profiles.
  - `cargo perf-soak --minutes=<N> -- <command>` repeats a workload and reports each printed metric's slope, so a leak shows up as growth instead of one sample.
  - `crates/proto/src/payload_budget.rs` holds serialized-size ceilings for the remote-editing messages, and they run in the normal test job.
- Optimization ground rules: never edit a benchmark to hit a target, never run two benchmarks concurrently, no `RUSTFLAGS`/`target-cpu=native` heroics, keep measured cases independent. No gate enforces any of this.
- Priority order for build and runtime knobs is compute, memory, binary size, then compile time — `docs/src/development/build-pipeline.md`.

## Rust / GPUI
- `.rules` is the authority for Rust style, GPUI context/entity rules, task cancellation, test timers, and panic policy. Read it before editing any crate; do not restate it here.
- Repo-specific additions only: `clippy.toml` disallows `std::process::Command::*`, `smol::Timer::after`, `serde_json*::from_reader`, and `cocoa::foundation::NSString::alloc`; `[workspace.lints]` in `Cargo.toml` records the promotion bar for new lints.
- `.cargo/config.toml` sets `MIMALLOC_ALLOW_THP=0` and the `MIRIFLAGS` that let `cargo miri test` run; an explicit env var wins over both.

## i18n
- Localize user-facing text with `i18n::tr(cx, key, fallback)` (`cx: &App`; some crates alias it, e.g. `app_i18n::tr`).
- Add every key to both `assets/locales/en.json` and `assets/locales/zh-CN.json`; `check locale-keys` requires identical key sets, `check locale-placeholders` requires matching format placeholders.
- The catalogs are authoritative for what is displayed. `check locale-fallbacks` requires the in-code `fallback` to equal the `en.json` value, so a changed fallback alone fails the gate instead of silently drifting.
- Do not localize brand names, shell commands, HTTP headers, or internal/log identifiers.

## Contributions
- DCO, no CLA: commit with `git commit -s`; `check dco` enforces sign-off on recent non-merge commits.
- `check philosophy` keeps the removed commercial/network surfaces absent and `check upstream-ledger` validates both decision ledgers. Both are gates — do not weaken them to land a change.
- Reusable patterns validated in a PR go into `.rules` (which has a high bar for new rules), and PR text carries a `Suggested .rules additions` section.
