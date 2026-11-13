---
title: Coding Standards
description: "Which file owns which rule, what a gate enforces, and where a new rule goes."
---

# Coding Standards

ZZZ writes its rules in six files that serve different audiences. This page
says which one owns what, so a rule lives in one place and a change lands in
one place.

## The map

| File                               | Audience                | Owns                                                                                                                |
| ---------------------------------- | ----------------------- | ------------------------------------------------------------------------------------------------------------------- |
| `.rules`                           | agents and humans       | Rust and GPUI rules: panic policy, entity/context rules, task cancellation, test timers, PR hygiene, rules hygiene. |
| `AGENTS.md`                        | agents                  | Repository layout, the gate commands, i18n, contribution mechanics. It adds only what `.rules` does not cover.      |
| `docs/.rules`                      | docs authors and agents | Voice, tone, formatting, terminology for user-facing documentation.                                                 |
| `docs/AGENTS.md`                   | docs automation         | mdBook structure, preprocessor syntax, anchors, and the phases of an automated docs update.                         |
| `.github/PULL_REQUEST_TEMPLATE.md` | contributors            | The fields a pull request must carry; `script/check-pr-metadata` reads them.                                        |
| `CONTRIBUTING.md`                  | contributors            | What gets merged, the AI and human review requirements, the MSRV policy, the UI/UX checklist.                       |

Two files are machine configuration rather than prose, and both are
enforced by gates instead of review alone:

| File                             | Enforced by                                                                            |
| -------------------------------- | -------------------------------------------------------------------------------------- |
| `clippy.toml`                    | `cargo xtask clippy` (`disallowed-methods`, `msrv`)                                    |
| `Cargo.toml` `[workspace.lints]` | `cargo xtask check lints-inheritance`, plus the promotion bar recorded in that section |

## One authoritative copy

A rule is written once and linked everywhere else. When a second file needs
to mention it, it points at the owner:

```markdown
Rust and GPUI rules live in `.rules`; read it before editing a crate.
```

Restating a rule in two files is the failure mode this page exists to
prevent: the copies drift, and the reader cannot tell which one the gate
enforces. The exception is a pointer that names the specific rule, which
costs nothing and survives a rewrite of the original.

## Where a new rule goes

| The rule is about              | It goes in                                                        |
| ------------------------------ | ----------------------------------------------------------------- |
| Rust or GPUI code              | `.rules`, through the process in its "Rules Hygiene" section      |
| One crate's traps              | `crates/<crate>/.rules`                                           |
| User-facing prose              | `docs/.rules`                                                     |
| Repository commands or layout  | `AGENTS.md`                                                       |
| A pull request field           | `.github/PULL_REQUEST_TEMPLATE.md` and `script/check-pr-metadata` |
| Merge policy or review process | `CONTRIBUTING.md`                                                 |

`.rules` has a deliberately high bar for new entries — non-obvious,
repeatedly encountered, and specific enough to act on — and agents propose
additions in the pull request body instead of editing the file. Everything
else is ordinary documentation work.

## External baselines

ZZZ does not invent its own Rust style. Four published standards are the
baseline, and `rustfmt.toml`, `clippy.toml`, and `[workspace.lints]` apply
most of them without a reviewer in the loop.

| Standard                                                                                   | Covers                                                                                                                       |
| ------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------- |
| [Rust Style Guide](https://doc.rust-lang.org/style-guide/)                                 | Layout. `rustfmt.toml` pins 100 columns, and `cargo fmt --all -- --check` runs in CI.                                        |
| [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)                         | Naming and interface contracts (`C-CONV`, `C-GETTER`, `C-CTOR`, `C-COMMON-TRAITS`, `C-NEWTYPE`, `C-CUSTOM-TYPE`, `C-DEBUG`). |
| [Google Rust Style Guide](https://google.github.io/styleguide/rustguide.html)              | Code organization, imports, ownership defaults, `unsafe`, `#[non_exhaustive]`, panic policy.                                 |
| [Microsoft Pragmatic Rust Guidelines](https://microsoft.github.io/rust-guidelines/) 2026.6 | Lint policy, error types, builders, crate and module layout, the FFI boundary, AI-facing code.                               |

Checked on 2026-11-13. Microsoft's page reports version 2026.6 and was
generated on 2026-10-26; Google's page carries no date, so re-read it before
quoting a rule from it.

Clippy's default `style` group covers most of the API Guidelines naming rules,
and `cargo xtask clippy` passes `--deny warnings`, so they are already hard
failures. The rest of this section records what ZZZ takes from the Google and
Microsoft documents, because those two overlap and disagree in places and a
silent choice would let the same argument return every review. The adopted
rules live in `.rules` under "External baseline rules"; what follows is why
each one was taken, altered, or dropped.

### Adopted

| Baseline item                                                                    | Landed as                                                                                               |
| -------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| Google "Don't use glob imports"; Microsoft `M-NO-PRELUDE`, `M-NO-GLOB-REEXPORTS` | `.rules`. 168 module-scope glob imports and two prelude modules predate the rule and stay.              |
| Microsoft `M-LINT-OVERRIDE-EXPECT`                                               | `.rules`. 263 `#[allow]` sites predate the rule and stay.                                               |
| Google "Unsafe code"; Microsoft `M-UNSAFE`, `M-UNSOUND`                          | `.rules`, plus `clippy::missing_safety_doc` and `clippy::multiple_unsafe_ops_per_block` already denied. |
| Google's two `#[non_exhaustive]` rules; Microsoft `M-PUBLIC-DEBUG`; `C-DEBUG`    | `.rules`.                                                                                               |
| Microsoft `M-DOCUMENTED-MAGIC`                                                   | `.rules`. A comment that explains why a value has that value is a `why` comment, not the banned kind.   |
| Microsoft `M-STATIC-VERIFICATION` lint list                                      | A candidate list only. Promotion still has to clear the per-lint bar in `[workspace.lints]`.            |

### Altered

- Google "Avoid `Rc` and `Arc`". ZZZ keeps `Arc` for `SharedString`, entity
  handles, and shared services: GPUI state is app-owned and cloning a refcount
  is how a handle is passed. What survives is the objection to shared state,
  not to the pointer: no `Rc<RefCell<T>>` standing in for an `Entity`, and no
  clone where a borrow works.
- Google `#[non_exhaustive]` applies to "a public API used by other teams".
  Every workspace crate is `publish = false`, so the trigger here is "another
  workspace crate matches or constructs it".
- Google's Chromium flat-file layout and Microsoft's `M-SMALLER-CRATES`. ZZZ
  already has a flat `crates/*/src/*.rs` layout with no `mod.rs`, so the rule
  that survives is the existing one: new code goes into an existing file unless
  it is a new logical component.
- Microsoft `M-APP-ERROR` (applications may use `anyhow`) matches what ZZZ
  already does: `anyhow` at application boundaries, `thiserror` where a crate
  exposes a typed error.

### Rejected

- Google's `[chrome]` items: no `async`/`.await`, no `std::thread`,
  `chromium::import!()`, and one crate per abstraction. ZZZ is async throughout
  on a GPUI dispatcher, and its crate boundaries follow compile-time cost
  rather than Chromium's build graph.
- Google "Reporting errors" (`panic!` for a violated invariant) and Microsoft
  `M-PANIC-ON-BUG`. A GUI that panics on a violated invariant loses the open
  document, so `.rules` keeps `?` and `.log_err()` as the production path and
  confines `unwrap`/`expect` to tests and proven invariants. Google's ban on
  `panic::catch_unwind` already holds here, which makes Microsoft's
  `M-PANIC-CONTINUATION` moot.
- Microsoft `M-MIMALLOC-APPS` and `M-TARGET-CPU`. These are packaging and
  deployment decisions owned by `cargo xtask build`, not source rules.
- Microsoft `M-FFI-NAMING` (`-sys`/`-ffi` suffixes). ZZZ's FFI lives in
  `crates/gpui_macos`, `gpui_windows`, `media`, `video`, and `sqlez`; renaming
  those buys a suffix nobody reads.
- High-assurance advice (global `#![forbid(unsafe_code)]`, no heap allocation,
  no dynamic dispatch). ZZZ is a desktop application with platform FFI and an
  allocation-heavy editor core.

### Sources that are not normative

- Firefox has no Rust style page. The old
  `code-quality/coding-style/coding_style_rust.html` returns 404; the tree now
  carries `writing-rust-code/` (Basics, Rust/C++ interop, Update Policy,
  XPCOM), which documents interop with Gecko, not style.
- Ferrocene is a TÜV SÜD qualified toolchain that ships the Ferrocene Language
  Specification, a language definition rather than a team style guide. High
  Assurance Rust is a personal open-source book. Neither is an industry coding
  standard, and neither is cited as one.
- The Tokio and Hyper lint sets are their own repository configuration, not
  published guidance. A lint borrowed from them enters through the same
  promotion bar as any other candidate.

## What is machine-enforced

`cargo xtask check` is the list (`GATES` in
`tooling/xtask/src/tasks/check.rs`), and `cargo xtask clippy` covers the
compiler-backed rules. The rules that still rely on review:

- Wording and tone in documentation; Prettier only checks formatting.
- Whether a test actually tests the behavior it names.
- Whether a change is one thing or three.

A rule that moves from review to a gate is a good change: it costs one gate
and removes a recurring review comment. Add the gate with the doc update so
the page does not claim a check that does not exist.

## Related

- [Testing](./testing.md)
- [Releasing ZZZ](./releasing.md)
- [Upstream Cherry-Pick](./upstream-cherrypick.md)
