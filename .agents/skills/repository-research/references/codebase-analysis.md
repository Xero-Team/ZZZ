# Codebase and Architecture Analysis

Use this reference to turn a checkout into an implementation-backed architecture
map. The goal is a decision-relevant trace, not an exhaustive file catalog.

## Contents

- Reading order
- Static survey and search strategy
- Representative traces by system type
- AST, symbol, lexical, and semantic evidence
- Monorepos and generated code
- Architecture note format

## Reading order

Inspect in this order because each layer constrains the next:

1. `AGENTS.md`, repository rules, security policy, contribution guide.
2. README, architecture/design documents, package manifests, workspace files.
3. Executable, library, server, extension, action, or MCP entrypoints.
4. Core domain types and interfaces.
5. State, persistence, queues, caches, and lifecycle code.
6. Representative tests, fixtures, benchmarks, and evaluation datasets.
7. Deployment, CI, release, telemetry, authentication, and network boundaries.

README text establishes declared intent. Implementation and tests establish
behavior. Generated API docs establish surface area but rarely prove execution
paths.

## Static survey and search strategy

Generate a survey before deep reading:

```bash
python .agents/skills/repository-research/scripts/survey_repositories.py \
  --sources .tmp/skill_ref/SOURCES.tsv \
  --output-dir .tmp/skill_ref/surveys
```

Use the survey as a map, then search adaptively:

- entrypoints: `main`, `cli`, `server`, `app`, `extension`, `action`, `handler`;
- boundaries: `trait`, `interface`, `protocol`, `adapter`, `provider`, `plugin`;
- data: `state`, `store`, `cache`, `database`, `index`, `checkpoint`, `queue`;
- retrieval: `parse`, `chunk`, `symbol`, `search`, `embed`, `rank`, `retrieve`;
- orchestration: `plan`, `supervisor`, `agent`, `tool`, `workflow`, `graph`;
- evidence: `source`, `citation`, `reference`, `provenance`, `trace`, `eval`;
- operations: `auth`, `token`, `sandbox`, `telemetry`, `network`, `retry`.

Use `rg --files`, `rg -n`, `git ls-files`, `git log`, and `nl -ba` first. Use an
AST/indexer when names are overloaded, symbol edges matter, or textual search
cannot distinguish definitions from references.

## Representative traces

### Code understanding or coding agent

Trace:

```text
workspace/repository discovery
→ ignore and language detection
→ parsing/chunking/symbol extraction
→ lexical/semantic index writes
→ query planning and retrieval
→ ranking/deduplication/context budget
→ model/tool request
→ cited answer, patch, or edit validation
```

Questions that distinguish implementations:

- Is the unit a file, text chunk, AST node, symbol, or graph document?
- How are definitions, references, and imports represented?
- How are indexes invalidated after edits or branch changes?
- Is retrieval lexical, semantic, hybrid, graph-based, or model-directed?
- Where are source spans and commit identity retained?
- How does the system handle unsupported languages and generated files?
- Does the edit loop run formatting, linting, tests, or patch verification?

### Deep Research system

Trace:

```text
user request and clarification
→ research brief or plan
→ query decomposition/delegation
→ search, browser, local files, or MCP tools
→ filtering, source curation, and deduplication
→ durable notes or context compression
→ report synthesis and citations
→ factual/quality evaluation and retry policy
```

Check whether concurrency is bounded, task state is resumable, tool failures are
visible, citations survive compression, and evaluation uses independent evidence
rather than self-approval alone.

### Repository health or analytics tool

Trace:

```text
URL/local input and ref resolution
→ metadata/tree/file loading
→ independent checks or metric collection
→ normalization and weighting
→ grade/comparison
→ table, JSON, Markdown, issue, or chart output
```

Identify ecosystem bias, missing-data handling, sampling, and whether the score
measures repository hygiene, activity, popularity, code correctness, or some
combination.

### MCP server or protocol

Trace transport initialization, capability negotiation, input schemas, path and
network validation, tool implementation, result schemas, error translation, and
session cleanup. A reference server demonstrates protocol use; it is not proof
of production hardening.

## AST, symbol, lexical, and semantic evidence

These layers answer different questions:

| Layer | Strong for | Weak for |
| --- | --- | --- |
| File/path search | exact names, configuration, known concepts | renamed or conceptual matches |
| Lexical index | identifiers and code phrases at scale | semantic paraphrases |
| AST/Tree-sitter | definitions, syntax units, language-aware chunks | cross-file semantics without extra indexing |
| SCIP/compiler index | definitions, references, implementations, typed symbols | unsupported languages and runtime behavior |
| Embeddings/vector DB | conceptual recall, natural-language questions | exact symbol edges and freshness guarantees |
| Graph/call analysis | dependency and flow questions | dynamic dispatch, reflection, generated/runtime wiring |

Prefer hybrid retrieval for broad exploration, then read the actual files.
Vector similarity is a candidate generator, not architecture evidence by itself.

## Monorepos and generated code

Determine workspace boundaries before scoring file counts or test coverage.
Record packages, default members, independently deployed services, shared
libraries, generated clients, vendored code, examples, fixtures, and assets.

Exclude generated/vendor/assets from architecture counts unless the question is
about generated surface area or distribution size. Follow the generator or
schema as the authoritative source when possible.

For large monorepos, choose a vertical slice that crosses the layers relevant to
the decision. Do not infer every package's behavior from the root README.

## Architecture note format

For every candidate, capture:

```text
Boundary:
Entrypoints:
Representative flow:
Core types/interfaces:
State and persistence:
Extension seams:
Security/network boundary:
Validation evidence:
Observed limitations:
Key citations:
```

A useful architecture note explains why the design behaves as observed and what
tradeoff follows. A directory listing without a flow or decision implication is
only preliminary evidence.
