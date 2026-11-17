---
title: Crate Map
description: "How the crates are classified, where per-crate rules live, and why there is no CODEOWNERS file."
---

# Crate Map

The workspace has 206 crate directories under `crates/`, plus `vendor/`,
`extensions/`, and the tooling crates. They are not one kind of code, and
the rules that apply to them differ. This page says which kind a crate is
and where a rule about it belongs.

## Categories

| Category          | Examples                                              | What it means                                                                                                    |
| ----------------- | ----------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| Upstream-shadowed | `gpui`, `editor`, `project`, `workspace`, `ui`, `vim` | Mirrors a crate of the same name in Zed. Absorbed commits land here, so local edits conflict with the next sync. |
| Local-original    | `telegram`, `telegram_ui`                             | Written here. No upstream counterpart, so the sync never touches it.                                             |
| Vendored          | `vendor/`, `crates/grammars/vendor/`                  | Third-party snapshots. Excluded from most gates; see each gate's pathspecs.                                      |
| Extension surface | `extension_api`                                       | The wasm ABI extensions compile against. A change here breaks published extensions.                              |
| Tooling           | `tooling/xtask`, `tooling/compliance`                 | Build and release automation. Not shipped in the app.                                                            |

To classify a crate, check whether Zed has `crates/<name>` upstream. If it
does, treat a local change as a divergence the next sync will fight over,
and record it in the sync report.

## Versions

| Version  | Crates                                            |
| -------- | ------------------------------------------------- |
| `0.1.0`  | Every workspace member without a reason to differ |
| `0.2.2`  | `gpui`                                            |
| `0.8.0`  | `extension_api`                                   |
| `1.29.0` | `zzz`                                             |

`crates/zzz` is the only version that follows upstream Zed, because it is
the package the release tag is derived from; see
[Releasing ZZZ](./releasing.md). The crate versions exist to satisfy cargo,
not to promise a compatibility window: every workspace member sets
`publish = false`.

## Per-crate rules

`.rules` says a rule that applies to a single crate belongs in that crate's
own `.rules` file, which agents read alongside the root one. Today that is
only `docs/.rules`, plus `docs/AGENTS.md` for the documentation toolchain.

The empty slot is deliberate. `.rules` has a high bar — non-obvious,
repeatedly encountered, and specific enough to act on — and a crate-level
file that restates the root rules or describes the module layout goes stale
faster than the code it describes. Add one when a crate has a trap that
reviewers keep explaining:

```text
crates/<crate>/.rules
```

Write the trap, not the map: what goes wrong, and what to do instead.

## Why there is no CODEOWNERS

Review routing by path assumes reviewers per path. This repository's history
is authored overwhelmingly by one maintainer, so a `CODEOWNERS` file would
assign nearly every path to the same person and reduce to a notification
duplicate. Revisit this if ownership spreads; the classification above is
what a CODEOWNERS file would be built from.

## Related

- [Coding Standards](./coding-standards.md)
- [Glossary](./glossary.md)
- [Upstream Cherry-Pick](./upstream-cherrypick.md)
