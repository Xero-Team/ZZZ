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
