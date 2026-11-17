---
name: committing-changes
description:
  "Splits a working tree into reviewable, signed-off commits with messages that
  explain why. Use when the user asks to commit, to split mixed changes, to write
  or fix a commit message, or when script/check-dco fails. Do not use for pushing,
  opening pull requests, releases, or upstream absorption."
license: CC-BY-4.0
compatibility: opencode
version: 1.0.0
metadata:
  author: Xero Team <maintainers@example.com>
  tags:
    - git
    - commits
    - dco
  prompt-language: English
  user-language: Simplified Chinese
---

Follow `REFERENCE.md` in this skill first. It carries the DCO rule, the message
shape the history already uses, and the staging rules. A commit that mixes a fix,
a reformat, and a dependency bump cannot be reviewed, reverted, or bisected.

# Committing Changes

You turn the working tree into commits that each carry one logical change, a
signed-off trailer, and a subject that says what changed and a body that says why.
This repository enforces the sign-off, so an unsigned commit is a gate failure, not
a style preference.

## Purpose

Leave history a reviewer can read one commit at a time, and a maintainer can revert
or bisect without unpacking unrelated work.

## Communication Rules

- Keep this skill prompt written in English.
- Interact with the user in Simplified Chinese.
- Write commit subjects and bodies in English, in the imperative mood.
- When a technical term could be ambiguous, add a short Chinese gloss followed by
  the English term in parentheses.

## When to Use

- The user asks to commit, commit this, or split the changes into commits.
- The user asks for a commit message, or an existing message needs fixing.
- `cargo xtask check dco` or `script/check-dco` fails.

Do not use this skill to push, to open or update a pull request, to cut a release,
to amend published history, or to land upstream syncs (that is
`absorbing-upstream`, which owns its own `sync:` commits).

## Operating Rules

- Commit only when the user asks. A finished task is not a request to commit.
- Never push, never force-push, never rewrite history the user did not name.
- Sign every commit: `git commit -s`. The DCO gate reads the trailer, and a
  missing `Signed-off-by` blocks the change.
- Stage by path: `git add -- <files>`. Do not run `git add -A` or `git add .` on a
  tree that holds more than the change you are committing.
- One logical change per commit. Keep docs, formatting-only edits, dependency bumps,
  and the behavioral change in separate commits unless the user says otherwise.
- The subject says what changed; the body says why. Never enumerate files in the
  body as a substitute for a reason.
- Match the subject style already in `git log`: a lowercase scope prefix and an
  imperative clause, for example `editor: Build completion sort keys once per
  match`. Do not invent a new prefix or sprinkle conventional-commit types.
- Do not commit generated or local state: `target/`, `.perf-runs/`, editor
  settings, or a `.env` file. If one appears staged, unstage it and say so.
- Verify before committing with the narrowest check that covers the change
  (`cargo xtask clippy -p <crate>`, `cargo test -p <crate>`, the touched
  `cargo xtask check <gate>`), and report what actually ran.
- Never rewrite, reformat, or add unrelated files to make the commit look tidier.

## Prerequisites

- A repository with configured `user.name`, `user.email`, and DCO sign-off enabled
  by `git commit -s`.
- `git`, and the pinned toolchain when the change needs a check.
- A clean understanding of which changes belong to this task, from
  `git status --porcelain` and `git diff`.

## Procedure

1. Read `README.md` (contribution section), `AGENTS.md`, `.rules` (pull request
   hygiene applies to the PR that follows, not to the commit subject), and this
   skill's `REFERENCE.md`.
2. Inspect before touching anything: `git status --porcelain`, `git diff --stat`,
   and `git log --oneline -10` for the local subject style.
3. Classify the changed paths into logical commits. Separate the behavioral change,
   tests that belong to it, docs, and unrelated deletions or reformatting.
4. Run the narrowest verification for each group and record the result. A commit
   whose crate does not build should not be created.
5. For each group in order: `git add -- <paths>`, then `git diff --staged` to
   confirm exactly those hunks, then commit with a subject and a why-body.
6. Run `script/check-dco` (or `cargo xtask check dco`) at the end and confirm the
   new commits carry the trailer.
7. Report the commits created, the checks that ran, and anything deliberately left
   uncommitted.

## Examples

```text
Commit this, but keep the locale catalogs separate from the panel change.
```

```text
check dco failed on my last commit; fix it without rewriting history I pushed.
```

## Troubleshooting

- `check dco` names a commit without a trailer: amend only if the commit is
  unpushed and the user agrees, using `git commit --amend -s`. Otherwise add a
  follow-up commit and say why the history stays as it is.
- A group will not build on its own: merge it into the commit that supplies its
  callers, or reorder. Do not leave a commit that only builds after the next one.
- `git add -- <paths>` picks up a hunk the user did not intend: unstage it with
  `git restore --staged <path>` and stage by hunk instead.
- The diff contains a secret or local config file: unstage it, leave it in the
  working tree, and tell the user. Do not commit it and do not delete their file.
- The subject needs a scope that does not exist yet: use the crate or area name in
  lowercase, as the history does, rather than inventing a taxonomy.

## Limitations

- This skill commits locally. It does not push, open pull requests, or fill in the
  `Release Notes:` body that `.rules` requires for a PR.
- The DCO gate covers recent non-merge commits; older history is not rechecked, so a
  passed gate is not proof that the whole branch is signed.
- Splitting a tangled diff can require staging by hunk. Where that is impractical,
  report the mixing instead of forcing an artificial split.

## Final Response

Summarize in Simplified Chinese:

- each commit created, as `<short sha> <subject>`, in order
- which paths went into which commit, and what was deliberately left uncommitted
- checks run and their PASS/FAIL, including `script/check-dco`
- anything the user should decide, such as an unpushed commit that needs signing
