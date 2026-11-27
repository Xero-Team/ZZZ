# Absorbing Upstream Reference

Normative rules for the `absorbing-upstream` skill.

## Contents

Read `Session Defaults` and `Classification` first; every batch needs them. Then
read only the sections the current step needs.

| If you are | Read |
|---|---|
| deciding whether a commit belongs in ZZZ | `Philosophy`, `Allow`, `Reject` |
| judging a commit that mixes upstream plumbing with a feature | `Isolation Test` |
| running the pick | `Git Commands`, `Apply` |
| checking the result | `Verification` |
| writing the budget report | `Report` |
| recording a rejection | `Rejection Ledger` |
| about to import rejected machinery | `Hard Stops` |
| looking for a precedent | `Worked Patterns` |

## Session Defaults

Override only when the user supplies a value.

```text
LAST_REVIEWED_UPSTREAM=831229fe70bbb126d49cde0b703ae6a8b25893d8
LOCAL_BASE_BRANCH=main
LOCAL_BASE_COMMIT=b3a1def633
UPSTREAM_URL=https://github.com/zed-industries/zed.git
UPSTREAM_REF=refs/heads/main
WORK_BRANCH=sync/upstream-YYYY-MM-DD
BATCH=first 20 commits
REPORT=docs/src/development/upstream/upstream-sync-YYYY-MM-DD.md
CREATE_REMOTE=no
CREATE_PR=no
PUSH=no
```

As of 2026-11-26, local `main` was `b3a1def633`. The last new upstream
window is `docs/src/development/upstream/upstream-sync-2026-11-26.md` on
`sync/upstream-2026-11-26`; it reviews `f00a94318c..831229fe70`, the first
twenty commits after batch F, and advances the reviewed baseline to
`831229fe70`. The live head at query time was `e4fa1642`, forty-five
commits later, so those stay unreviewed until the next run. The preceding
window is `docs/src/development/upstream/upstream-sync-2026-11-24.md`,
which reviews `f4fe0be1f8..f00a94318c`, the first twenty commits after
batch E. Before that,
`docs/src/development/upstream/upstream-sync-2026-11-22.md` reviews
`54fff2ff5b..f4fe0be1f8`, the seven commits that landed after batch E, and
`4a7d10473a..54fff2ff5b` is the 2026-11-20-e report, which drained the
backlog that batches B through D left behind, after the
`6ceb6fee87..4a7d10473a` window covered by the 2026-11-13, 2026-11-16,
2026-11-17, 2026-11-20, 2026-11-20-b, 2026-11-20-c and 2026-11-20-d
reports. The most recent full re-audit is
`docs/src/development/upstream/upstream-sync-audit-2026-11-17.md`; the tree
holds 60 sync reports and 6 audit reports. Query live upstream again at run
start; never trust these recorded values as the live head.
The next range starts after `LAST_REVIEWED_UPSTREAM`.

## Philosophy

From the root `README.md`:

> Some things should not be configurable. They should simply be absent.
>
> Strip away the telemetry, the upsells, the proprietary coupling.
> What remains is the editor.

ZZZ is local-first, no-account, ACP-only.

| Surface | Rule |
| --- | --- |
| Telemetry / analytics | Absent in source. Never reintroduce. |
| Sign-in, account, billing, plan | Absent. No chips, trials, or cloud UX. |
| Native / proprietary Zed Agent | Reject. ACP only. |
| Commercial model APIs | Silent manual configuration only. |
| Default AI endpoint | Local infrastructure. No signup path. |
| Collaboration / shared threads | Reject. |
| CLA | Do not add. Sign commits with DCO (`-s`). |

AI may stay when it is provider-agnostic and points at infrastructure
the user owns. OpenAI-compatible, Anthropic-compatible, Ollama,
llama.cpp, and other silent manual providers are in scope. Copilot
auth, ChatGPT or Zed subscription routes, and native agent UI are not.

If a hunk is only safe after importing a rejected account, collab,
telemetry, or native-agent path, drop the whole commit.

## Classification

Every candidate gets exactly one class.

| Class | Meaning | Local commit |
| --- | --- | --- |
| A | Complete safe absorption, or already-equivalent local behavior. | Yes if code changes. |
| B | Port only isolatable, philosophy-safe behavior onto current ZZZ APIs. | Yes. Never claim sync without it. |
| C | Rejected: philosophy, missing architecture, unisolatable rewrite, or upstream-only infra. | No. Leave the tree clean. |

- Already-equivalent A: no code change. Name the local equivalent.
- Clean A: keep the upstream patch intact.
- Conflicting A: `git cherry-pick --abort`, then become B or C.
- B is not "liked hunks plus a new unused API." If omitted hunks are
  required for the retained invariant, the commit is C.

## Allow

- Editor, buffer, git, LSP, project panel, terminal, vim/helix
- Platform compatibility and stability
- Performance of existing local paths
- ACP and MCP behavior that stays ACP-only
- Provider-agnostic model support and silent manual provider UI
- Grammars, syntax, and settings that already exist in ZZZ
- Docs that describe behavior ZZZ actually ships

## Reject

- Telemetry, analytics, `telemetry::event!`, data-collection flags
- Sign-in, sign-up, usernames, young-account logic, cloud-only UX
- Trial, billing, subscription, upgrade, plan chips
- Copilot auth/settings/credentials
- ChatGPT or Zed subscription providers and account-bound routes
- Native agent panel, sandbox, thread, terminal, sidebar
- Collaboration panel, contact finder, shared-thread cloud features
- Staff/server feature flags and Preview/Stable flag rollouts
- Guild/community/triage/CI/release/marketing automation
- Upstream release metadata, npm/docs-theme/benchmark plumbing
- Promotional external-agent or Zed Business docs
- Dependency or lockfile-only churn with no independent ZZZ behavior
- New public APIs with no current ZZZ caller
- Absent crates or protocols: `crates/path`, AccessKit writer path,
  `MergeBaseWithWorktree`, V4 `udiff`, WebGL backend, external-drag
  GPUI APIs, MultiWorkspace rewrite

Known local divergences that usually force B or C:

- ZZZ deleted `editor/src/input.rs` and consolidates some editor and
  terminal modules
- ZZZ has `paths`, not upstream `crates/path`
- Tests use `smol::channel`, not undeclared `async_channel`
- Feature-flag policy differs; several upstream flags are absent
- Markdown preview, remote transport, and some GPUI drag/scroll APIs
  have already diverged
- Project-panel undo/redo is enabled on all channels locally

## Isolation Test

A commit is portable only if all of these are true:

1. The retained behavior is philosophy-safe.
2. A real ZZZ caller exists, or the change is a complete local bugfix
   on an existing path.
3. It does not require an absent crate, protocol variant, or GPUI API.
4. It does not require a rejected account, collab, telemetry, or native
   agent surface.
5. After omitting rejected hunks, the safety invariant still holds and
   can be tested, or the omission is explicitly test-only.
6. It does not depend on an unabsorbed earlier commit.

If any check fails, classify C. Do not import an unused type so a later
commit can land.

A dry-run cherry-pick conflict caused by divergent file layout is
evidence for B or C, not a license to force the upstream shape.

## Git Commands

Do not add a named remote.

```sh
git status --porcelain
git rev-parse --short HEAD
git ls-remote "$UPSTREAM_URL" "$UPSTREAM_REF"
git fetch --no-tags "$UPSTREAM_URL" "$UPSTREAM_REF"
git rev-parse FETCH_HEAD
git log --reverse --format='%H %s' \
  "${LAST_REVIEWED_UPSTREAM}..FETCH_HEAD"
```

Abort if the worktree is dirty for reasons other than the report being
written.

## Apply

**A, complete and clean**

```sh
git cherry-pick -x -s <sha>
```

Keep the upstream message.

**A, already equivalent**

Make no code change. Name the local function, type, or commit. Do not
reintroduce a deleted upstream path.

**A, conflict**

```sh
git cherry-pick --abort
```

Then port as B or reject as C.

**B**

Port the smallest local equivalent. Match existing ZZZ names and APIs.
Commit with `git commit -s`:

```text
sync: <imperative summary> from <8-char sha>

Upstream: <full 40-char sha>

Retained: <what local behavior now does>
Omitted: <what was dropped, and why>
```

If `cargo check` on the touched crate fails, revert every file from
that attempt and reclassify C.

**C**

Change no product code. Write the rejected surface, missing API, or
isolation failure. "Looks unrelated" is not a reason.

## Verification

```sh
git diff --check
cargo fmt --all -- --check
cargo check --locked -p <touched_crate>
cargo test --locked -p <crate> <test_filter>
```

Do not run `cargo test --workspace` unless asked. Use `cargo xtask clippy`
if you lint, never `cargo clippy`.

This host is Linux. Inspect macOS and Windows hunks, but record those
runtime tests as `NOT RUN`.

Do not fix unrelated baseline failures. Confirm a failure predates the
change, on a clean checkout, before calling it baseline.

If Prettier or `cargo fmt` rewrites an unrelated region, restore it.

## Report

Create or append `REPORT` using the 2026-08-07 shape:

1. Scope: local base, upstream URL/ref, reviewed head, live head,
   query time, range start.
2. Decisions table: `Upstream | Class | Local commit | Disposition`.
3. Applied work: every direct A used `git cherry-pick -x -s`; every B
   has `Upstream` / `Retained` / `Omitted`.
4. Per-commit narrative for anything that was not a clean A.
5. Verification with `PASS` / `FAIL` / `BLOCKED` / `NOT RUN` /
   `CONFLICT`.

```sh
cd docs && npx --yes prettier@3.9.10 --write src/development/upstream/<report>.md
cd docs && npx --yes prettier@3.9.10 --check src/development/upstream/<report>.md
```

`3.9.10` is the version pinned in `script/prettier`, which
`cargo xtask check docs` runs over all of `docs/`; a scoped write must use
the same version or the check will disagree. Read `PRETTIER_VERSION` from
`script/prettier` if the pin moves. Do not run Prettier across all of
`docs/src/`. Advance the reviewed baseline only through commits you
classified.

## Decision Ledgers

One backfill writes both ledgers from the report decision tables:
`docs/src/development/upstream-ledger.tsv` for the A and B decisions, and
`docs/src/development/upstream-rejected.tsv` for the C decisions. After
writing the report, run:

```sh
./script/backfill-upstream-ledger
cargo xtask check upstream-ledger
```

Do not hand-edit either ledger; improve the report and regenerate. The
checker fails on a stale ledger, a malformed row, a duplicate SHA prefix, a
SHA listed in both files, a recent row whose absorption no local commit
proves, or a rejection row whose commit was actually absorbed. Known
exceptions are listed in `script/upstream-ledger-allowlist`.

The `verify` column is why the absorbed ledger exists. For the newest three
batches that absorbed anything, a row must be `verified` (an `Upstream:` or
`cherry picked from commit` trailer names the SHA), or `equivalent` (the
report states the local tree already matched upstream). Anything else is
`unverified` and fails the check, so record which case applies while the
evidence is still in front of you.

## Hard Stops

Stop and leave the tree clean if:

- `LAST_REVIEWED_UPSTREAM` is missing or not an ancestor of
  `FETCH_HEAD`
- a cherry-pick or merge is stuck and `--abort` is required
- a B port cannot compile without a rejected or absent API
- the next commit needs an unabsorbed prerequisite classified C

Record the SHA, the reason, and the last reviewed baseline.

## Worked Patterns

From the 2026-08-07 audit. Classification anchors only. Do not absorb
these SHAs again.

- Clean A: `20ce54f8` worktree path pairing, landed as `70674296`.
- Already-equivalent A: `f851d82e` left-biased cursor. No V4 `udiff`
  route was restored.
- Isolatable B: `027cf0de` Markdown scrollbar setting, ported as
  `673c0c6f` after a conflicting cherry-pick.
- Failed B then C, later absorbed: `c7aea6cb` and `41c0f28b` failed
  `cargo check` on missing APIs and were reverted, then landed as
  `e39b426b14` and `772132ec1d` once those APIs arrived.
- Philosophy C: `d356b2f5` subscription compaction, `66ed3027` native
  agent panel, `cdf3ccd0` telemetry in extensions.
- Unisolatable C: `200fb85c` bracket cache, `2318f45f` MultiWorkspace
  rewrite, `790dcefb` Sweep prompt bound to an absent API.
