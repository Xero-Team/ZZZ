# Working-tree notes

- Baseline command before the research workspace was initialized: ZZZ had no tracked modifications and reported only `?? .agents/skills/repository-research/` and `?? .tmp/`. The requested reference corpus already lived under the untracked `.tmp/` tree.
- This research first created `.tmp/gui-framework-research/` inside that already-untracked `.tmp/` tree, then moved the completed artifacts into `docs/src/development/gui-framework-research/` at the user’s request. Before that move, it did not change tracked ZZZ files.
- All nine reference checkouts reported a clean worktree.
- All nine reference checkouts are shallow. The `zed` checkout is additionally sparse and includes GPUI-related crate paths only; repository-wide Zed documentation and unrelated crates were not available locally.
