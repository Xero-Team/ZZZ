# Repository investigation: <decision>

Checked at: <UTC timestamp>

Corpus: `sources.tsv`, `inventory.tsv`, `evidence.tsv`, and `experiments.tsv`

## Decision answer

State the recommended choice for the concrete workload and the decisive evidence.

## Scope and evidence limits

List inspected revisions, substitutions, unavailable sources, skipped runtime tests, and date-sensitive facts.

## Architecture findings

### <candidate>

- Product boundary and entrypoints:
- End-to-end execution path:
- State, persistence, and extension seams:
- Security and operational boundary:
- Evidence: `<repository>:<path>:<locator>@<commit>`

## Comparison matrix

| Dimension | Candidate A | Candidate B | Evidence and limits |
| --- | --- | --- | --- |

## Tradeoffs and risks

Cover licensing, maintenance, hosted dependencies, credentials, data boundaries, cost, and failure modes.

## Recommendation and falsification experiment

Describe the smallest reversible experiment, dataset, and success criteria that could change the recommendation.

## Unknowns

List unanswered questions requiring runtime tests, credentials, private data, or upstream clarification.

## Handoff to execution

Research ends at a decision; execution needs its own artifacts. When the recommendation is adopted, carry the result forward as:

- `refactoring-plan.md` — staged contract. Each stage names the evidence claims it relies on, its success threshold, and its rollback.
- `goal.md` — copyable execution instruction that points at the report, the plan, the ledgers, and the progress ledger.
- `progress-ledger.md` — per-stage changes, commands run, `PASS` / `FAIL` / `BLOCKED` / `NOT RUN`, commit SHAs, and the upstream sources used.

Re-pin the baseline in all three when `HEAD` moves. Port same-ancestry code through the repository's upstream absorption process instead of copying from a secondary fork.
