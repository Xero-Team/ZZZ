---
name: repository-research
description: "Produces evidence-backed repository research from source, history, and primary docs: architecture maps, comparisons, and due diligence. Use for technology selection and health reviews. Do not use for reviewing ZZZ's own code or diffs, auditing ZZZ's recorded upstream decisions, or porting upstream commits."
license: CC-BY-4.0
compatibility: opencode
version: 1.0.0
metadata:
  author: Xero Team <maintainers@example.com>
  tags:
    - research
    - repositories
    - evidence
  prompt-language: English
  user-language: Simplified Chinese
---

# Repository Research

Treat repository research as an evidence problem. Pin the inspected revision,
trace representative behavior through implementation code, and keep observed
facts separate from documentation claims, inference, and unknowns.

## Purpose

Answer one decision question about one or more repositories with evidence a
reader can reproduce: a pinned corpus, a claim ledger, and a recommendation
that names the experiment able to falsify it.

## Communication Rules

Communicate results in Simplified Chinese unless the user requests another
language. Preserve product, protocol, and API names as written upstream.

## Choose the relevant guidance

Read only the references needed for the current request:

- For URL verification, substitutions, cloning, GitHub metrics, history depth,
  and corpus layout, read [references/acquisition.md](references/acquisition.md).
- For entrypoint discovery, architecture maps, data-flow tracing, monorepos,
  AST/index/RAG analysis, and code evidence, read
  [references/codebase-analysis.md](references/codebase-analysis.md).
- For comparison dimensions, normalization, weights, status labels, maintenance
  signals, and recommendations, read
  [references/comparison.md](references/comparison.md).
- For shared-ancestry, fork, and upstream-diff questions, read
  [references/lineage.md](references/lineage.md).
- For code-intelligence and Deep Research design patterns, browser/MCP roles,
  evaluation loops, and known tool boundaries, read
  [references/research-systems.md](references/research-systems.md).
- For claim ledgers, citations, confidence, report structure, and validation,
  read [references/evidence-reporting.md](references/evidence-reporting.md).

For multi-repository work, acquisition, codebase analysis, comparison, and
evidence reporting usually all apply. Research-system comparisons also need the
research-systems reference.

## Available scripts

All scripts are standard-library Python and operate read-only on cloned
repositories unless their name says `clone` or `init`. Run each with
`--help` before adapting its defaults.

| Script | Purpose | Arguments |
| --- | --- | --- |
| `scripts/init_workspace.py` | Create the workspace from `assets/` templates without overwriting existing files. | `--workspace` |
| `scripts/verify_sources.py` | Run non-interactive `git ls-remote` checks and write a verification ledger. | `--sources --output [--timeout] [--strict]` |
| `scripts/clone_sources.py` | Shallow-clone verified direct and substitute sources; refuses to overwrite non-repositories. | `--sources --output [--base-dir] [--depth] [--jobs] [--timeout] [--no-checkout] [--dry-run]` |
| `scripts/inventory_repositories.py` | Record commit, branch, shallow state, file and language counts, manifests, policy files, and working-tree status. | `--sources --output [--base-dir]` |
| `scripts/survey_repositories.py` | Generate deterministic static surveys with subsystem, manifest, dependency, entrypoint, test, documentation, and keyword hints. | `--sources --output-dir [--base-dir] [--max-file-bytes] [--skip-existing]` |
| `scripts/compare_lineage.py` | Compare shared paths across checkouts and write `lineage.tsv` counts. | `--sources --output [--base-dir] [--paths]` |
| `scripts/validate_evidence.py` | Validate paths, locators, commit pins, timestamps, evidence kinds, report citations, and a `lineage.tsv`. | `--sources --evidence [--report] [--lineage] [--base-dir] [--allow-missing-checkout]` |

`--skip-existing` on `survey_repositories.py` protects survey files a human has
annotated, and `--allow-missing-checkout` turns a deliberately cleaned-up corpus
into warnings instead of errors.

Prefer the templates in `assets/` for `sources.tsv`, `evidence.tsv`,
`experiments.tsv`, and the final report. Ledger filenames stay lowercase so a
workspace can sit beside the report it supports.

## Prerequisites

- `python3` (standard library only) and `git` on the host.
- Network access to the sources, or a pre-populated local corpus.
- Write access to a workspace directory outside the cloned repositories.

## Core workflow

1. **Frame the decision.** Identify the repositories, comparison question,
   audience, time sensitivity, operational constraints, and dimensions that can
   change the decision. State reasonable assumptions instead of blocking on
   optional details.

2. **Build the source ledger.** Initialize the workspace, record every requested
   item, and verify its resolved repository URL. Classify sources as `direct`,
   `substitute`, or `unavailable`. A topic page, deleted repository, search
   result, commercial service, or organization page is not a cloneable source.
   Never replace it silently.

3. **Create a reproducible corpus.** Clone into stable paths under the research
   workspace. Use shallow clones for current architecture; fetch the minimum
   additional history needed for evolution or activity questions. Record every
   inspected `HEAD` and leave upstream working trees unchanged.

4. **Inventory before interpreting.** Generate the inventory and static survey.
   When candidates share an ancestor, also generate `lineage.tsv` so a fork is
   not counted as an independent design. Read repository instructions, security
   and contribution documents, README and architecture documents, manifests,
   entrypoints, tests, benchmarks, and representative implementation modules in
   that order.

5. **Trace behavior end to end.** Follow one representative task from input to
   output. Code-understanding systems require discovery → parsing/indexing →
   retrieval/ranking → context assembly → model/tool call → answer/edit.
   Research systems require scope → planning → search/tools → evidence storage
   or compression → synthesis → evaluation. Health tools require input → checks
   → scoring → reporting/export.

6. **Record claims while reading.** Add material claims to `evidence.tsv` as
   soon as they are established. Cite implementation or tests for behavioral
   claims. Use documentation evidence only for declared intent or when source
   is unavailable, and label it accordingly.

7. **Compare equivalent dimensions.** Normalize the same workload and evidence
   standard across candidates. Mark cells `verified`, `partial`, `claimed`, or
   `unknown`; do not turn unavailable evidence into a zero. Keep stars and
   activity separate from architecture quality.

8. **Write the decision report.** Lead with the workload-specific answer, then
   show source limitations, architecture findings, the comparison matrix,
   tradeoffs, security/license/operations risks, unknowns, and the smallest
   experiment that could change the recommendation. When the answer is adopted,
   hand it off as an execution plan, a goal, and a progress ledger, using the
   templates in `assets/report-template.md`.

9. **Validate.** Run the evidence validator and recheck that substitutions are
   visible, dynamic metrics are timestamped, local claims use the pinned commit,
   and conclusions do not exceed the inspected evidence. List skipped runtime
   tests rather than implying they passed.

10. **Revalidate when the code moves.** A ledger describes one revision. On a
    later re-pin, update `sources.tsv`, `inventory.tsv`, and `evidence.tsv`
    together, re-read every decisive claim, and record the drift in a dated
    report section instead of overwriting the earlier conclusion. See
    [references/evidence-reporting.md](references/evidence-reporting.md).

## Non-negotiable evidence rules

- Pin each local architecture claim to a repository, path, locator, and commit.
- Separate `implementation`, `test`, `documentation`, `external-primary`,
  `inference`, and `unknown` evidence.
- Treat README statements, benchmark tables, and vendor comparisons as claims
  until independently reproduced or supported by inspectable artifacts.
- Timestamp GitHub/API metrics and name the endpoint or primary source.
- Do not install dependencies or execute cloned code during baseline static
  research. Run a separate, explicitly scoped experiment when static evidence
  cannot answer the question.
- Do not expose credentials, local secrets, private repository contents, or raw
  sensitive payloads in reports or generated inventories.

## Examples

```text
Compare three Rust code-indexing projects and recommend one for our monorepo.
```

```text
Is this fork an independent design or a re-badged upstream? Pin both HEADs.
```

## Troubleshooting

- A source URL does not resolve: record it as `unavailable` in
  `sources.tsv` instead of substituting silently, and name the substitute.
- `validate_evidence.py` rejects a locator: re-read the file at the pinned
  commit; headings and line numbers drift after a re-pin.
- A shallow clone cannot prove ancestry: state the depth in the report and
  cite the object that exists locally rather than claiming history.
- A GitHub metric cannot be reproduced later: timestamp it and name the
  endpoint in `evidence.tsv`.

## Limitations

- Static reading cannot establish runtime behavior; benchmark and latency
  claims need a scoped experiment.
- Private repositories, rate limits, or missing credentials cap coverage;
  record the gap instead of extrapolating.
- Star counts and activity are not architecture quality, and the ledgers
  describe one revision each.

## Completion criteria

The work is complete when the corpus can be reproduced from the ledgers, each
material conclusion has valid evidence or is explicitly unknown, all candidates
were assessed against the same relevant dimensions, and the recommendation
names both its target workload and the experiment that would falsify it.
