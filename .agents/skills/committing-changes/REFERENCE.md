# Reference: ZZZ Commit Mechanics

Normative material for `committing-changes`. Commands run from the repository root.

## Contents

- The sign-off requirement
- Message shape
- What is staged
- Verification before a commit
- Out of scope
- Sources

## The sign-off requirement

`README.md` and `.github/PULL_REQUEST_TEMPLATE.md` require `git commit -s`, and
`script/check-dco` enforces it.

```sh
git commit -s -m "<subject>" -m "<why>"
```

- `-s` appends `Signed-off-by: <user.name> <user.email>` to the trailer block.
  Configure both before committing; an unset identity produces a trailer the
  project cannot attribute.
- `cargo xtask check dco` runs `script/check-dco` with its defaults.
- `script/check-dco [COMMIT_COUNT] [GIT_DIR]` defaults to the last 50 non-merge
  commits in the current repository.
- The check looks for the `signed-off-by:` trailer case-insensitively. It does not
  compare the name or address to the commit author, so a wrong identity passes the
  gate and is still wrong.
- Merge commits are excluded, because the upstream-sync workflow creates them
  without a per-commit trailer.
- Exit codes: `0` all signed, `1` at least one unsigned, `2` bad arguments or not a
  git repository.

If a recent commit is unsigned and unpushed, `git commit --amend -s` fixes it. Ask
before amending, and never amend or rebase a commit the user already pushed.

## Message shape

The history uses three lines of convention, visible in `git log --oneline`:

```text
editor: Build completion sort keys once per match
sync: fold watcher paths once when matching ancestors from 99c890d3
docs(upstream): record the 2026-11-16 sync through 596dfdcf32
build: bump dependencies and follow merman's 0.10 engine API
```

- **Scope prefix**, lowercase, naming the crate or area: `editor`, `git_ui`,
  `agent`, `docs`, `build`, `review`. Reuse one that already exists. The
  parenthesized form (`docs(upstream):`) appears where a scope needs a sub-area;
  it is still a scope, not a conventional-commit type.
- **Imperative clause** after the colon: "Build", "Fix", "Move", "Stop", "Record".
  A reader should be able to finish "This commit will ___".
- **No conventional-commit types** (`feat:`, `fix:`, `chore:`). `.rules` also bans
  them in pull request titles, and the history does not use them.
- **Body explains why**, wrapped near 72 columns, with a blank line after the
  subject. Name the constraint, the failure it prevents, or the upstream change it
  follows. If the subject already says everything, a one-line commit is fine.
- **Reference a commit or issue** when the change only makes sense in its context,
  as the `sync:` commits do with "from `<sha>`".

The subject is not a file list, and the body is not a changelog. A reviewer reads
the diff for what changed and the message for why.

## What is staged

- Stage by path: `git add -- crates/agent/src/history.rs assets/locales/en.json`.
- Use `git add -p` when a file mixes two logical changes.
- Never `git add -A` or `git add .` on a mixed tree.
- Confirm with `git diff --staged` before committing. The staged snapshot is the
  commit; the working tree is not.
- Keep out: `target/`, `.perf-runs/`, local editor settings, `.env`, credentials,
  and any generated artifact. If one is already staged, `git restore --staged
  <path>` and leave the file on disk.
- A file the change makes unused, such as an import or a helper, is removed in the
  same commit. Pre-existing dead code stays for a separate change.

## Verification before a commit

Match the check to the change, and report the actual result:

```sh
cargo xtask clippy -p <crate>        # lint for one crate
cargo test -p <crate>                # behavior-affecting change
cargo fmt --all -- --check           # formatting is a gate
cargo xtask check                    # compiler-free gates, or a single one:
cargo xtask check locale-keys
cargo xtask check philosophy
```

A commit that cannot build on its own breaks bisection. If a group only builds
after a later commit, merge the two or reorder so each commit stands alone.

`git log --oneline -10` before writing the first subject. The local phrasing is the
style guide; the reference above is a summary, not a replacement.

## Out of scope

- Pushing, force-pushing, and opening pull requests.
- The `Release Notes:` body section and the AI/human review labels, which
  `.rules` and `script/check-pr-metadata` apply to a PR rather than to a commit.
- Upstream absorption. `absorbing-upstream` creates `sync:` commits with the
  `from <sha>` suffix and its own reporting.
- Release tagging.

## Sources

- Repository: `README.md`, `CONTRIBUTING.md`, `AGENTS.md`, `.rules`,
  `script/check-dco`, `.github/PULL_REQUEST_TEMPLATE.md`,
  `tooling/xtask/src/tasks/check.rs`.
- Adapted from `commit-craft` in
  https://github.com/alirezarezvani/claude-skills (MIT). The one-logical-change
  split and the stage-by-path rule come from there; the DCO trailer, the scope
  prefix style, and the gate commands are ZZZ's, so its Python planner and secret
  scanner are not bundled.
