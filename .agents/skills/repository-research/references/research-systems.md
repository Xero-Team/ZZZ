# Repository-Research System Patterns

Use this reference when assessing codebase assistants, Deep Research agents,
browser agents, MCP servers, repository analytics, or when designing a combined
repository-investigation stack.

## Contents

- Four-layer capability model
- Code-intelligence patterns
- Deep Research patterns
- Browser and MCP boundaries
- Evaluation patterns
- Failure modes and design implications

## Four-layer capability model

Repository investigation usually needs four separable layers:

```text
repository and platform facts
→ structural/code intelligence
→ research orchestration
→ evidence validation and evaluation
```

No single open-source project should be assumed to implement all four well.
Identify whether a candidate is a product, agent harness, retrieval library,
protocol, metric service, or benchmark before comparing it.

## Code-intelligence patterns

### Compact symbol maps

Aider-style repository maps extract definitions/references with Tree-sitter,
rank relevant symbols, cache results, and fit the map to a token budget. This is
strong for interactive editing and cheap global orientation. It is weaker for
persistent cross-repository evidence and compiler-grade relationships.

### Hybrid local search

Bloop-style systems combine a lexical engine, vector database, local embeddings,
and Tree-sitter symbol information. Hybrid retrieval improves recall across
exact identifiers and conceptual questions, but requires index freshness,
resource management, and explicit source-span preservation.

### Multiple index interfaces

Continue-style indexers separate full-text, code-snippet/Tree-sitter, chunk, and
vector indexes behind a common lifecycle. This supports replacement and
incremental refresh, while increasing configuration and migration complexity.

### Protocol-level symbol facts

SCIP-style indexes use a language-neutral schema for definitions, references,
implementations, and documents. They are useful as a stable fact layer beneath
search or agents. A protocol does not supply semantic retrieval, orchestration,
or a user-facing report by itself.

### Issue-to-change agents

Sweep-style pipelines join repository parsing, lexical/vector search, issue
interpretation, code modification, and PR workflows. They demonstrate an end
to end change loop, but their architecture assumptions may be coupled to model,
embedding, GitHub App, or hosted service versions.

## Deep Research patterns

### Explicit state graphs

Open Deep Research-style graphs model clarification, brief writing, supervisor,
researcher tools, note compression, and final synthesis as named states. This
makes retries, observability, and resume behavior clearer than an implicit
while-loop.

### Agent harnesses

Deep Agents-style harnesses bundle planning, subagents, filesystem, shell,
context compression, persistence, skills, and MCP around a graph runtime. They
are suitable orchestration bases, while permissions and data boundaries still
belong in tools and sandboxes.

### Source curation and report roles

GPT Researcher-style designs separate query planning, retrievers, scraping,
source curation, context management, and writing. Separation helps testing and
provider replacement. Inspect whether citation identity survives each stage.

### Iterative RAG

DeepSearcher-style systems load files or sites, split chunks with references,
embed them into selectable vector databases, then generate follow-up queries,
retrieve, rerank, deduplicate, reflect, and synthesize. They fit private document
research, but generic chunks do not replace symbol-aware code facts.

### Minimal breadth/depth recursion

Small recursive research agents generate a bounded set of queries, process
results concurrently, derive learnings and new directions, recurse by depth,
and write a report. They are valuable baselines for understanding behavior and
cost, but need stronger provenance, recovery, and security for production use.

### Model and self-evolution research

Model-centric repositories may publish inference loops, training pipelines,
optimizers, resource lifecycle/version protocols, benchmarks, and weights. Do
not score them as turnkey repository-analysis products unless the deployment,
tools, citations, and runtime boundary are actually present.

## Browser and MCP boundaries

Browser agents are best used when APIs, Git/raw content, and static documents
cannot expose the required material. DOM/accessibility serialization and CDP or
Playwright actions handle dynamic sites, discussions, and authenticated pages,
but add volatility, prompt injection, credential, origin, CAPTCHA, and timing
risks.

MCP standardizes how tools and resources are exposed. Reference Git and
filesystem servers demonstrate capability schemas and path controls; they do
not automatically provide GitHub PR/Issue/star data or production hardening.
Inspect transport, roots, allowlists, authentication, result size, error
translation, and session cleanup.

Prefer this acquisition order:

```text
local Git/files → primary API → raw/static documentation → browser automation
```

## Evaluation patterns

Repository-health tools provide explainable file/layout checks and comparison
reports. Their scores usually measure hygiene or project conventions, not code
correctness or architecture quality.

Deep Research benchmarks can assess completeness, analytical depth,
instruction following, readability, and factual consistency using task-specific
criteria and reference reports. LLM judges must be versioned, calibrated, and
separated from the agent under test where possible.

For repository research, evaluate at least:

- retrieval recall against a hand-labeled source set;
- citation precision and whether locators resolve at the pinned commit;
- unsupported-claim rate;
- repeatability of the retrieved evidence set;
- context tokens, model/tool calls, wall time, and external cost;
- degradation under missing network, unsupported languages, generated files,
  branch changes, and large monorepos.

## Failure modes and design implications

| Failure | Design response |
| --- | --- |
| README treated as implementation | Evidence kinds and implementation citations |
| Embeddings miss exact symbols | Hybrid lexical/symbol/vector retrieval |
| Symbol index misses concepts | Semantic search plus direct file reading |
| Context compression loses citations | Durable evidence ledger outside chat context |
| Parallel agents duplicate work | Shared task/evidence IDs and deduplication |
| Live metrics drift | Timestamp endpoints and separate them from source facts |
| Browser content changes | Prefer APIs; save bounded primary evidence |
| Tool can access too much | Filesystem roots, origin allowlists, sandbox policy |
| Benchmark becomes marketing | Reproduce or label first-party claims |
| Substitute is mistaken for original | Source status displayed in every report |
