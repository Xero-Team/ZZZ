---
name: absorbing-upstream
description:
  "Absorbs Zed upstream commits into ZZZ with A/B/C cherry-pick
  classification. Use when cherry-picking, syncing upstream, absorbing
  Zed main, porting an upstream SHA, classifying commits as A/B/C, or
  writing docs/src/development/upstream-sync-*.md."
license: CC-BY-4.0
compatibility: opencode
metadata:
  prompt-language: English
  user-language: Simplified Chinese
---

Follow `REFERENCE.md` in this skill first. If philosophy and isolation
conflict with a clean cherry-pick, reject or port; never import rejected
machinery to make the trees match.

# Absorbing Upstream

You selectively absorb Zed `main` into ZZZ. This is not a merge and not
a fast-forward. ZZZ keeps fork-only absences.

## Communication Rules

- Keep this skill prompt written in English.
- Interact with the user in Simplified Chinese.
- Write reports, commit messages, and git trailers in English.
- When a technical term could be ambiguous, add a short Chinese gloss
  followed by the English term in parentheses.

## When to Use

- The user asks to cherry-pick, absorb, or sync upstream Zed commits.
- The user names an upstream SHA and wants it landed in ZZZ.
- The user wants a new or continued `docs/src/development/upstream-sync-*.md`.

Do not use this skill for ordinary ZZZ feature work, or for Zed
release-channel cherry-picks via `script/cherry-pick`.

## Operating Rules

- Conservative porter, not a merger.
- Classify every candidate `A`, `B`, or `C`. No skips.
- Do not re-review commits at or before `LAST_REVIEWED_UPSTREAM`.
- Do not add an `upstream` remote, push, open a PR, or run
  `script/cherry-pick`.
- Do not import unused APIs, lockfile churn, or scaffolding for later.
- If a B port fails `cargo check`, revert the attempt completely and
  reclassify `C`.

## Procedure

1. Read `README.md`, `AGENTS.md`, the latest
   `docs/src/development/upstream-sync-*.md`, and this skill's
   `REFERENCE.md`.
2. Resolve session inputs from the user, else use the defaults in
   `REFERENCE.md`.
3. Preflight: clean worktree, query live upstream with `git ls-remote`
   and `git fetch --no-tags` against the URL, never a named remote.
4. Stop if `LAST_REVIEWED_UPSTREAM` is not an ancestor of `FETCH_HEAD`.
5. Create or continue `WORK_BRANCH` from `LOCAL_BASE_BRANCH`.
6. Walk the batch oldest-first. For each SHA, read the full diff and
   the current ZZZ call chain, then apply the class rules in
   `REFERENCE.md`.
7. Verify each applied change with the narrowest `cargo check` /
   `cargo test` on touched crates.
8. Append the run to `REPORT` and format only that file with Prettier.
9. Regenerate the rejection ledger with
   `./script/backfill-upstream-ledger` and verify it with
   `./script/check-upstream-ledger`.

## Final Response

Summarize in Simplified Chinese:

- reviewed range and live upstream head
- A / B / C counts
- local commits created
- last reviewed baseline
- any hard stop and why
