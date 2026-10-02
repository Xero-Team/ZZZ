# Corpus Acquisition and Live Metrics

Use this reference when the task involves remote repositories, URL resolution,
substitutions, history, GitHub activity, or reproducible local checkouts.

## Contents

- Source states and substitutions
- Workspace layout and schemas
- Verification and cloning
- Choosing history depth
- Live GitHub metrics
- Failure and safety rules

## Source states and substitutions

Classify every requested item before analysis:

| Status | Meaning | Treatment |
| --- | --- | --- |
| `direct` | The requested repository resolves to the inspected source | Compare normally |
| `substitute` | The requested item is unavailable or non-cloneable and a same-category repository was selected | Display both names and the reason; never imply identity |
| `unavailable` | No defensible open source was found | Preserve the row and mark comparison cells unknown |

Common non-repositories include GitHub topic/search pages, organization pages,
documentation sites, product landing pages, package registries, deleted repos,
and commercial services whose source is not public.

A substitute should match the capability being studied, not merely share a
keyword. Record the missing capability if the substitute covers only part of
the original claim.

## Workspace layout

Recommended layout:

```text
.tmp/skill_ref/
  SOURCES.tsv
  VERIFIED_SOURCES.tsv
  CLONE_RESULTS.tsv
  INVENTORY.tsv
  EVIDENCE.tsv
  REPORT.md
  surveys/
  <stable-repository-slug>/
```

`SOURCES.tsv` fields:

```text
id, requested_name, requested_url, resolved_url, clone_path, status, notes, checked_at
```

Use stable lowercase slugs. Keep clone paths inside the workspace unless the
user explicitly points to an existing local checkout.

## Verification and cloning

Preferred sequence:

```bash
python .agents/skills/repository-research/scripts/init_workspace.py --workspace .tmp/skill_ref
python .agents/skills/repository-research/scripts/verify_sources.py \
  --sources .tmp/skill_ref/SOURCES.tsv \
  --output .tmp/skill_ref/VERIFIED_SOURCES.tsv --strict
python .agents/skills/repository-research/scripts/clone_sources.py \
  --sources .tmp/skill_ref/VERIFIED_SOURCES.tsv \
  --output .tmp/skill_ref/CLONE_RESULTS.tsv
python .agents/skills/repository-research/scripts/inventory_repositories.py \
  --sources .tmp/skill_ref/SOURCES.tsv \
  --output .tmp/skill_ref/INVENTORY.tsv
```

Verification uses `git ls-remote --symref <url> HEAD` with terminal prompting
disabled. A successful HTTP page load does not prove that Git transport works.

The clone helper deliberately has no force mode. For an existing Git checkout,
compare the configured origin with the resolved URL and report a mismatch. For
an existing non-repository path, stop for that row rather than deleting it.

Use shallow, single-branch, no-tag, blob-filtered clones for current-state
architecture work. Record that the checkout is shallow. Reading a filtered blob
may trigger a network fetch; offline reproducibility therefore requires either
fully materializing the cited files or archiving the final evidence set.

## Choosing history depth

Use only the history needed by the question:

- **Current architecture:** depth 1 is normally enough.
- **Maintenance/activity:** query the GitHub API and/or fetch enough commits to
  cover a stated time window.
- **Evolution or regression:** fetch the relevant tag, commit range, or merge
  base rather than unshallowing everything.
- **Ownership/contributor concentration:** use full history only when the metric
  materially affects the decision and explain bot filtering and time window.

Do not compare last-commit dates from shallow clones as if they represented the
same time window or release policy.

## Live GitHub metrics

Static source and live platform metrics are different evidence classes. Record
UTC retrieval time and endpoint for:

- repository stars, forks, watchers, archived/disabled state, default branch;
- latest release and tag dates;
- open/closed issue and PR counts or sampled resolution times;
- contributor concentration and recent commit activity;
- dependency advisories or workflow status.

Prefer primary APIs. Examples:

```text
GET /repos/{owner}/{repo}
GET /repos/{owner}/{repo}/releases/latest
GET /repos/{owner}/{repo}/commits?since=<timestamp>
GET /repos/{owner}/{repo}/issues?state=all&since=<timestamp>
```

Document pagination, sampling, bot exclusion, and rate-limit effects. GitHub's
`open_issues_count` includes pull requests; do not label it as issue count
without filtering.

Never use stars as a quality score. Trends can support adoption analysis, while
architecture, correctness, security, and maintenance require separate evidence.

## Failure and safety rules

- Set `GIT_TERMINAL_PROMPT=0`; never hang waiting for credentials.
- Do not include tokens in URLs, command output, ledgers, or reports.
- Keep private repository paths and contents out of user-visible artifacts
  unless the user explicitly requested them.
- Treat rate limits and authentication failures as missing evidence, not zero.
- Record redirects and renamed repositories; keep the originally requested URL.
- Do not execute hooks, installers, build scripts, or repository binaries during
  acquisition.
