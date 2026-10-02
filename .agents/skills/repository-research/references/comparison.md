# Competitor Comparison and Decision Rubric

Use this reference for multi-repository evaluations, build-versus-adopt choices,
and technology selection. Compare candidates against a shared workload and
shared evidence threshold.

## Contents

- Normalize the workload
- Evidence status labels
- Default dimensions
- Scoring and weights
- Maintenance and maturity
- Recommendation rules

## Normalize the workload

Write a concrete scenario before scoring. Include:

- corpus size, language mix, monorepo shape, and private/public boundary;
- query types: symbol lookup, architecture explanation, PR review, broad Web
  research, or longitudinal metrics;
- required outputs: cited answer, patch, chart, benchmark, or decision report;
- latency, offline, model/provider, deployment, and budget constraints;
- security constraints for source code, browser sessions, and credentials;
- success criteria that can be reproduced across candidates.

Do not compare a protocol, library, product, benchmark, and hosted service as if
they offered the same boundary. State where composition is required.

## Evidence status labels

Use one status per matrix cell:

| Status | Meaning |
| --- | --- |
| `verified` | Implementation or test evidence directly supports the capability |
| `partial` | The core exists but coverage, integration, or operational behavior is incomplete |
| `claimed` | First-party documentation claims it; implementation was not verified |
| `unknown` | Evidence was unavailable, ambiguous, private, or outside the static investigation |

`unknown` is not zero. A substitute inherits no evidence from the requested
project. A capability verified in a library does not prove the surrounding
hosted product uses it in the same way.

## Default dimensions

Select only dimensions that matter to the decision. The full catalog is:

| Dimension | Inspect |
| --- | --- |
| Acquisition | local/GitHub inputs, refs, auth, incremental sync |
| Code intelligence | AST, symbols, references, language coverage |
| Retrieval | lexical, semantic, hybrid, graph, reranking, filters |
| Freshness | invalidation, branch changes, live index updates |
| Orchestration | planning, loops, subagents, concurrency, cancellation |
| Evidence | path/line citations, provenance, deduplication, confidence |
| Tool boundary | GitHub, MCP, browser, shell, filesystem, sandbox |
| Persistence | caches, indexes, sessions, checkpoints, resumability |
| Extensibility | providers, plugins, skills, adapters, schemas |
| Evaluation | tests, benchmarks, datasets, quality/cost/latency metrics |
| Operations | install, deployment, observability, retries, failure modes |
| Security/privacy | path and origin limits, credentials, telemetry, retention |
| License/governance | license clarity, contributor terms, hosted restrictions |
| Maintenance | releases, current development, issue/PR handling, bus factor |
| Cost | model/API/storage/compute requirements and predictable limits |

Popularity and marketing reach may be a separate adoption dimension, never a
proxy for implementation quality.

## Scoring and weights

Qualitative comparison is preferred when evidence is uneven. When a score helps
selection, use:

- `0`: verified absent for the normalized workload;
- `1`: claimed or very thin implementation;
- `2`: useful but incomplete or constrained implementation;
- `3`: implemented and evidenced for the normalized workload;
- `N/A`: dimension does not apply to this product boundary;
- `?`: unknown evidence.

Never convert `?` to `0`. Show raw statuses and notes alongside numbers.

Choose weights before examining winners. Keep weights traceable to user needs.
A reasonable total is 100, with no single dimension above 30 unless it is a
hard requirement. Apply hard gates separately, for example:

```text
must run locally
must preserve source citations
must support Rust and TypeScript
must avoid mandatory hosted accounts
```

A candidate failing a hard gate should not win through unrelated points.

Record proposed and completed runtime checks in `EXPERIMENTS.tsv` using the
same workload, revision, dataset, metric, and success threshold for every
candidate. Set thresholds before running the experiment, and keep failed or
inconclusive results rather than replacing them with a cleaner rerun.

## Maintenance and maturity

Use multiple signals over a stated time window:

- current archived/read-only status;
- release cadence and time since last release;
- commits affecting core code, not only docs/dependencies;
- open PR/issue age distributions and maintainer responses;
- contributor concentration and organization continuity;
- CI, security policy, release provenance, and upgrade practices;
- compatibility statements and deprecated surfaces.

Avoid invented thresholds such as “no commit in 90 days means abandoned.”
Project type and release policy matter. A stable protocol may change slowly;
an agent tied to rapidly changing model APIs may require frequent maintenance.

Distinguish implementation maturity from product maturity. A large repository
can contain experiments, generated assets, abandoned modules, and hosted-only
components. A small repository can be intentionally complete.

## Recommendation rules

A defensible recommendation contains:

1. the exact workload and constraints;
2. the decisive strengths with evidence;
3. the tradeoffs and missing capabilities;
4. why the alternatives lose for this workload;
5. composition needs, migration cost, and lock-in;
6. the smallest experiment that could change the decision.

Prefer conditional recommendations when workloads differ:

```text
Choose A for local symbol-aware editing.
Choose B for private-document semantic retrieval.
Combine protocol C with orchestrator D when cross-language citations are mandatory.
```

Do not name a universal winner when the compared boundaries differ.
