---
name: repository-research
description: "Investigates GitHub and local repositories from source code, history, metadata, and primary documentation, then produces evidence-backed architecture analyses and competitor comparisons. Use for repository due diligence, multi-repo technology selection, codebase RAG assessment, maintenance or health reviews, and deep implementation comparisons; skip for superficial README summaries or ordinary code edits."
license: CC-BY-4.0
metadata:
  prompt-language: English
  user-language: Simplified Chinese
  output-target: .agents/skills
---

# Repository Research

Treat repository research as an evidence problem. Pin the inspected revision,
trace representative behavior through implementation code, and keep observed
facts separate from documentation claims, inference, and unknowns.

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
- For code-intelligence and Deep Research design patterns, browser/MCP roles,
  evaluation loops, and known tool boundaries, read
  [references/research-systems.md](references/research-systems.md).
- For claim ledgers, citations, confidence, report structure, and validation,
  read [references/evidence-reporting.md](references/evidence-reporting.md).

For multi-repository work, acquisition, codebase analysis, comparison, and
evidence reporting usually all apply. Research-system comparisons also need the
research-systems reference.

## Reusable scripts

All scripts are standard-library Python and operate read-only on cloned
repositories unless their name explicitly says `clone` or `init`.

- `scripts/init_workspace.py`: create a research workspace from templates
  without overwriting existing files.
- `scripts/verify_sources.py`: run non-interactive `git ls-remote` checks and
  write a verification ledger.
- `scripts/clone_sources.py`: shallow-clone verified direct/substitute sources;
  refuse to overwrite non-repositories and support `--dry-run`.
- `scripts/inventory_repositories.py`: record commit, branch, shallow state,
  file/language counts, manifests, policy files, and working-tree status.
- `scripts/survey_repositories.py`: generate deterministic static surveys with
  subsystem, manifest, dependency, entrypoint, test, documentation, and
  architecture-keyword hints.
- `scripts/validate_evidence.py`: validate local evidence paths, line/heading
  locators, commit pins, external timestamps, and evidence kinds.

Run each script with `--help` before adapting its defaults. Prefer the templates
in `assets/` for `SOURCES.tsv`, `EVIDENCE.tsv`, `EXPERIMENTS.tsv`, and the final report.

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
   Read repository instructions, security and contribution documents, README
   and architecture documents, manifests, entrypoints, tests, benchmarks, and
   representative implementation modules in that order.

5. **Trace behavior end to end.** Follow one representative task from input to
   output. Code-understanding systems require discovery → parsing/indexing →
   retrieval/ranking → context assembly → model/tool call → answer/edit.
   Research systems require scope → planning → search/tools → evidence storage
   or compression → synthesis → evaluation. Health tools require input → checks
   → scoring → reporting/export.

6. **Record claims while reading.** Add material claims to `EVIDENCE.tsv` as
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
   experiment that could change the recommendation.

9. **Validate.** Run the evidence validator and recheck that substitutions are
   visible, dynamic metrics are timestamped, local claims use the pinned commit,
   and conclusions do not exceed the inspected evidence. List skipped runtime
   tests rather than implying they passed.

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

## Completion criteria

The work is complete when the corpus can be reproduced from the ledgers, each
material conclusion has valid evidence or is explicitly unknown, all candidates
were assessed against the same relevant dimensions, and the recommendation
names both its target workload and the experiment that would falsify it.
