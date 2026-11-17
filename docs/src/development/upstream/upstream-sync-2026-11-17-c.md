---
title: Upstream Sync 2026-11-17 (batch C)
description: Selective Zed upstream sync, the three commits after the 2026-11-17 batch B.
---

# Upstream Sync 2026-11-17 (batch C)

## Scope

- Local base: `main` at `c00ed3e478`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `e5be9608ddd4cb0f2f179e1c0014f0a3a0337260`
- Live upstream head queried: `698368515d17a602dd51ea61297e5412ce06f52f`
- Query time: `2026-11-17`
- Reviewed range: `e5be9608dd..698368515d`, three commits, oldest first
- New reviewed baseline: `698368515d17a602dd51ea61297e5412ce06f52f`
- Remaining after this batch: none; the reviewed baseline is the live head

This closes the tail that `upstream-sync-2026-11-17-b.md` left open. None of the
three is portable as it stands: one conflicts with a locally diverged page
layout, one introduces a security subsystem ZZZ does not have, and one touches
only provider crates the ACP-only cleanup deleted.

Each disposition was decided by isolation rather than by reading the title:
the upstream diff was applied to an export of local `HEAD` with
`git apply --check --verbose`, and the failing hunks were split out and retested
one at a time.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                                                         |
| ---------- | ----- | ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 849b5b8e07 | C     | --           | Pinned page header for the first-run import page. Both hunks conflict, because the local file differs from the upstream pre-image by 412 diff lines (`i18n::tr`, `zed_actions` to `zzz_actions`), so the rewrite needs a hand port. |
| 3dcdee6d2b | C     | --           | Adds a Linux sandbox subsystem (`crates/sandbox_linux`) and rewires `crates/sandbox` and `crates/http_proxy`. Those crates are absent locally and `Cargo.toml` has no `sandbox` dependency, so the subsystem has nowhere to live.   |
| 698368515d | C     | --           | Adds a Gemini thinking level in `crates/google_ai`, `crates/language_models`, and `crates/language_models_cloud`. The ACP-only cleanup deleted every in-tree provider and the hosted model crates, so there is no local caller.     |

Totals: zero `A`, zero `B`, three `C`.

## Applied Work

None. No local commit, remote, pull request, or named upstream remote was
created for this batch.

## Per-Commit Narrative

### 849b5b8e07 — pinned header on the first-run import page (C)

The commit turns the page root into a `v_flex`, factors the shared column into a
`content_column()` helper, and moves the header out of the scrolling body so it
stays put while the body scrolls.

Both hunks fail (`:301`, `:335`). The local file is not a stale copy of the
pre-image: it differs by 412 diff lines, because ZZZ localized the page through
`i18n::tr`, renamed `zed_actions` to `zzz_actions`, and dropped the cloud and
agent-server imports with the ACP-only cleanup. The `Render` impl has to be
reworked by hand rather than patched, which is why this is `C` rather than a
trivial `B`.

### 3dcdee6d2b — new Linux sandbox (C)

The commit adds `crates/sandbox_linux` (36-line manifest, 173-line design note,
a 2280-line end-to-end test), rewrites `crates/sandbox` and its WSL/bubblewrap
test helpers, drops `crates/http_proxy`'s pinned-host module, and moves the typos
configuration from `typos.toml` to `.config/typos.toml`.

Local `crates/` has no `sandbox`, no `sandbox_linux`, and no `http_proxy` crate,
and `Cargo.toml` declares no `sandbox` dependency. Every product path in the
commit is missing, so no hunk has a target. Adopting the Linux sandbox would be a
new subsystem, not a sync.

### 698368515d — Gemini thinking level (C)

Every touched file is in `crates/google_ai`, `crates/language_models`, or
`crates/language_models_cloud`. `91a2911a68` deleted the in-tree provider crates
and `75e7e83732` deleted the rest of the AI stack, so all three paths are gone and
`git apply` reports `No such file or directory` for each.

## Verification

| Check                                                 | Result                                  |
| ----------------------------------------------------- | --------------------------------------- |
| `git log e5be9608dd..origin/main`                     | 3 commits, all 2026-11-17               |
| Isolation test for `849b5b8e07`                       | 0/2 hunks apply                         |
| Isolation test for `3dcdee6d2b`                       | 0 hunks; every product path absent      |
| Isolation test for `698368515d`                       | 0 hunks; every product path absent      |
| `./script/backfill-upstream-ledger` after this report | three new rows, existing rows unchanged |
| `cargo xtask check upstream-ledger`                   | PASS                                    |
