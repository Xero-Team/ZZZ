# Evidence Ledger and Reporting

Use this reference when recording claims, citing source code, validating a
comparison, or drafting the final deliverable.

## Contents

- Evidence schema and kinds
- Locator rules
- Claim construction
- Confidence and contradiction handling
- Validation
- Report structure

## Evidence schema and kinds

Use one TSV row per material claim:

```text
claim_id
repository
claim
evidence_kind
path_or_endpoint
locator
commit_sha
checked_at
confidence
notes
```

Allowed evidence kinds:

| Kind | Use |
| --- | --- |
| `implementation` | Executable source or configuration directly implements the behavior |
| `test` | Test, fixture, benchmark, or reproducible evaluation supports it |
| `documentation` | README/design/API docs declare intent or usage |
| `external-primary` | GitHub API, registry, release, official hosted endpoint |
| `inference` | Conclusion derived from multiple cited observations |
| `unknown` | Evidence was sought but unavailable or insufficient |

Do not cite generated search output as evidence. It only locates source files.
For an inference, list supporting claim IDs in `notes`. For an unknown, record
what was checked and what would resolve it.

## Locator rules

For local evidence, `path_or_endpoint` is relative to the repository checkout.
Allowed locators:

- `L42` or `L42-L67` for source lines;
- `#section-heading` for a Markdown heading;
- `file` only when the whole small file or manifest is the evidence.

Pin `commit_sha` to the inspected HEAD or an explicit historical revision. The
validator accepts a unique SHA prefix but reports mismatches.

For `external-primary`, use a complete endpoint or official URL and set
`checked_at` to an ISO-8601 UTC timestamp. Store bounded summaries rather than
credentials or large raw responses.

A user-facing citation can use:

```text
<repository>:<path>:<locator>@<short-sha>
```

When writing inside the local workspace, also use clickable absolute file links
in the final response when appropriate.

## Claim construction

A material claim should be atomic and decision-relevant:

Good:

```text
The indexer stores full-text records in SQLite and code chunks in LanceDB.
```

Weak:

```text
The project has strong indexing.
```

Split architecture, maintenance, benchmark, and hosted-service claims. A single
citation rarely proves all four.

For behavioral claims, prefer this order:

1. implementation plus test;
2. implementation alone;
3. test/benchmark with inspectable fixture and method;
4. first-party documentation;
5. explicitly labeled inference;
6. unknown.

## Confidence and contradictions

Use `high`, `medium`, or `low`:

- `high`: direct implementation/test evidence at the pinned revision;
- `medium`: partial implementation, clear documentation, or corroborated
  inference;
- `low`: ambiguous behavior, incomplete substitute, stale docs, or external
  state that was not reproducible.

When evidence conflicts, keep both claims and explain scope, version, feature
flag, product boundary, or documentation drift. Do not silently choose the more
favorable source.

First-party benchmark results remain `documentation` or `test` depending on
whether the corpus, runner, raw outputs, and scoring method are inspectable.
Label comparisons against competitors as vendor-authored unless independently
reproduced.

## Validation

Run:

```bash
python .agents/skills/repository-research/scripts/validate_evidence.py \
  --sources .tmp/skill_ref/SOURCES.tsv \
  --evidence .tmp/skill_ref/EVIDENCE.tsv
```

The validator checks:

- recognized repository IDs/names;
- valid evidence kinds and confidence values;
- local path containment and existence;
- line ranges and Markdown heading locators;
- commit pins against checkout HEAD;
- URL shape and timestamp for external-primary evidence;
- explanatory notes for inference and unknown evidence;
- duplicate or missing claim IDs.

A passing ledger proves structural consistency, not that the interpretation is
correct. Re-read decisive source locations before finalizing the report.

## Report structure

Start from `assets/report-template.md`. Keep this order:

1. **Decision answer** — recommendation for the normalized workload.
2. **Scope and evidence limits** — commits, date, substitutions, unavailable
   sources, and skipped runtime work.
3. **Architecture findings** — concise vertical traces per candidate.
4. **Comparison matrix** — same dimensions, statuses, and evidence threshold.
5. **Tradeoffs and risks** — security, privacy, license, maintenance, cost,
   operational dependencies, and failure modes.
6. **Recommendation and falsification experiment** — what to test next and the
   success criteria that could change the result.
7. **Unknowns** — only unresolved questions with material decision impact.

Keep raw command output, inventories, surveys, and API payload summaries in the
research workspace. The report should explain findings rather than reproduce
logs.

## Final review questions

- Does every decisive statement have a claim ID and resolvable source?
- Are substitutions visible wherever their evidence is used?
- Are dynamic metrics timestamped and separated from code facts?
- Are documentation claims labeled rather than written as observed behavior?
- Were candidates compared at equivalent product boundaries and workloads?
- Does the recommendation include a realistic falsification experiment?
