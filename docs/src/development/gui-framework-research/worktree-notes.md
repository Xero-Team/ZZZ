# Working-tree notes

- ZZZ was clean at `152a5eb983a883c69a6cc4eae082312ba75aa9f9` when this
  refresh started.
- The nine reference repositories were verified with non-interactive
  `git ls-remote` and shallow-cloned into `.tmp/ui_ref/` on
  `2026-10-03T11:28Z`.
- Every reference checkout has a clean worktree. The checkouts are shallow,
  single-branch, no-tag, and blob-filtered. They must remain read-only during
  the GPUI refactor.
- Zed is no longer a sparse checkout. The local tree contains the complete
  current checkout, while the research conclusions continue to use only GPUI
  and related platform paths as implementation evidence.
- Compared with the previous report baseline `34fb99e58d0`, current ZZZ changes
  in the inspected GUI area are limited to Editor teardown/cancellation work;
  `crates/gpui` and `ui_*` did not change.
- This refresh changes tracked research artifacts, adds the detailed execution
  plan and Goal instruction, and creates the untracked `.tmp/` corpus requested
  by the user.
