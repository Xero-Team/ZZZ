# Lineage and Fork Comparison

Use this reference when candidates share an upstream, when the target itself is a
fork, or when a comparison includes several forks of the same project. Without
it, three forks of one codebase read as three independent designs.

## What the ledger answers

`lineage.tsv` records, for each pair of checkouts, how much source is literally
shared:

| Field | Meaning |
| --- | --- |
| `repository_a`, `repository_b` | Repository IDs from `sources.tsv` |
| `head_a`, `head_b` | Inspected commit of each side |
| `path_filter` | Path prefixes compared, or `whole-tree` |
| `common_paths` | Relative paths present in both checkouts |
| `byte_identical` | Common paths whose blob hash matches |
| `changed_common_paths` | Common paths whose content differs |
| `only_a`, `only_b` | Paths present in one checkout only |

## How to run

```bash
python .agents/skills/repository-research/scripts/compare_lineage.py \
  --sources .tmp/skill_ref/sources.tsv \
  --output .tmp/skill_ref/lineage.tsv
```

Restrict to the subsystem under study when whole-tree counts are dominated by
assets, vendored code, or examples:

```bash
python .agents/skills/repository-research/scripts/compare_lineage.py \
  --sources .tmp/skill_ref/sources.tsv \
  --output .tmp/skill_ref/lineage-gpui.tsv \
  --paths crates/gpui,crates/editor
```

The script reads the tree at each checkout's `HEAD` with `git ls-tree`, so it
needs no blob fetch and works on blobless shallow clones. It never writes to the
checkouts.

## Interpretation rules

- **Identical bytes do not mean identical design.** Byte equality shows shared
  provenance, not who is right. Cite the behavior you actually read.
- **Compare like boundaries.** Restrict `--paths` so a fork's added platform
  crates do not inflate the shared ratio.
- **Read samples of `changed_common_paths`.** The count says divergence exists;
  only reading the diff says which side is ahead or whether the fork froze.
- **`only_b` is where independent value lives.** Extract those paths from the
  survey and read them before citing a fork as an innovation source.
- **Two forks are one witness.** Treat same-ancestry implementations as a single
  evidence vote; a second fork repeating upstream behavior does not corroborate
  it independently.
- **Exclude generated and vendored trees** from comparative counts unless the
  question is distribution size.

## Validation

The evidence validator also reads the lineage ledger:

```bash
python .agents/skills/repository-research/scripts/validate_evidence.py \
  --sources .tmp/skill_ref/sources.tsv \
  --evidence .tmp/skill_ref/evidence.tsv \
  --lineage .tmp/skill_ref/lineage.tsv
```

It rejects unknown repository IDs, non-integer counts, and rows where
`common_paths` does not equal `byte_identical + changed_common_paths`. A head pin
that the checkout no longer contains is reported, so a lineage ledger is not
silently read against a replaced corpus; a pin that exists locally but is not
the current `HEAD` is accepted, because the ledger records one revision.

## Reporting

State the ancestor revision and the `path_filter` next to any lineage claim. In
the report, use lineage rows to explain why a candidate is a reference and not
an alternative, then point to the specific `only_b` paths that justify reusing
any of its ideas.

When the shared code belongs to the target's own upstream, port it through the
repository's upstream absorption process instead of copying from a secondary
fork, so provenance and licensing stay traceable.
