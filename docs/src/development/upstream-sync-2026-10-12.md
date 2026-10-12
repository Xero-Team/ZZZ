---
title: Upstream Sync 2026-10-12
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-10-12

## Scope

- Target branch: `sync/upstream-2026-10-12`, branched from `main` at
  `9aed9c73aa`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `93d745668805029476e6d7fbb19652116fed51de`
- Reviewed head: `7eccdcecb5c5fe150a9e94f98b55af30b7d486b6`
- Live upstream head queried: `7eccdcecb5c5fe150a9e94f98b55af30b7d486b6`
- Query time: `2026-10-12T15:15:02+02:00`
- Reviewed range: the three commits after `93d7456688`, oldest first
- Remaining after this batch: 0 commits through the queried live head

The previous reviewed baseline is an ancestor of `FETCH_HEAD`. The range is
shorter than the twenty-commit batch because upstream advanced by only three
commits since the last query.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                       |
| ---------- | ----- | ------------ | ----------------------------------------------------------------------------------------------------------------- |
| fa1b94c6b9 | C     | --           | Adds a first-contribution GitHub workflow and comment script; community automation ZZZ does not ship.             |
| 51a3a0e971 | C     | --           | Rewrites `PathStyle::is_absolute`/`has_root` on the absent `crates/path` `Components` API; unisolatable refactor. |
| 7eccdcecb5 | B     | b7f6072ba4   | Font-family schema accepts GPUI's virtual aliases; ZZZ alias names substituted for the upstream ones.             |

Totals: zero `A`, one `B`, two `C`.

## Applied Work

- `b7f6072ba4` is a `B` port. `SettingsStore::json_schema` merges a
  `VIRTUAL_FONT_NAMES` list into the `FontFamilyName` enum so the aliases GPUI
  resolves are accepted by the settings schema. The list uses ZZZ's
  `.ZZZMono`, `.ZZZSans`, `ZZZ Plex Mono`, and `ZZZ Plex Sans` names, which
  `gpui::font_name_with_fallbacks` resolves, in place of the upstream
  `.ZedMono`, `.ZedSans`, `Zed Plex Mono`, and `Zed Plex Sans` names. Both
  upstream regression tests were kept.

## Rejected Work

- `fa1b94c6b9` adds `.github/workflows/welcome_first_contribution.yml`, a
  Slack failure-notification hook, and
  `script/github-welcome-first-contribution.py`; this is community automation
  that ZZZ keeps absent.
- `51a3a0e971` changes `PathStyle::is_absolute` to take a `&Path` and adds
  `has_root`, backed by the `crates/path` `Components::has_root` and
  `Components::prefix` API, then updates eighteen call sites across
  `acp_thread`, `agent`, `agent_ui`, `markdown_preview`, `project`, and
  `workspace`. ZZZ has no `crates/path` crate; its `util::paths::PathStyle` is
  a `&str`-based handcrafted implementation with no cross-platform
  `Components`, so neither the refactor nor its caller updates can be
  isolated. The commit requires an absent crate and is unisolatable.

## Verification

| Check                                               | Result  | Notes                                                                    |
| --------------------------------------------------- | ------- | ------------------------------------------------------------------------ |
| Live `git ls-remote` and `git fetch --no-tags`      | PASS    | `FETCH_HEAD=7eccdcecb5c5fe150a9e94f98b55af30b7d486b6`                    |
| Previous baseline ancestry                          | PASS    | `93d7456688` is an ancestor of `FETCH_HEAD`                              |
| `cargo check --locked -p settings`                  | PASS    | Font-alias schema port compiles                                          |
| `cargo test --locked -p settings virtual`           | PASS    | Two virtual-font tests pass                                              |
| `cargo fmt --all -- --check` and `git diff --check` | PASS    | Final source checks                                                      |
| Report Prettier                                     | PASS    | `npx prettier --check src/development/upstream-sync-2026-10-12.md`       |
| Rejection ledger backfill and check                 | PASS    | `./script/backfill-upstream-ledger` and `./script/check-upstream-ledger` |
| macOS and Windows runtime checks                    | NOT RUN | Host is Linux                                                            |
| `cargo test --workspace`                            | NOT RUN | Not requested                                                            |

The general reviewed baseline advances to
`7eccdcecb5c5fe150a9e94f98b55af30b7d486b6`, the queried live head.
