---
title: Upstream Sync 2026-10-02
description: Targeted migration of Zed's current Mermaid rendering stack.
---

# Upstream Sync 2026-10-02

## Scope

- Target branch: `sync/upstream-2026-10-02-mermaid` from local `main` at
  `9c5299c08eebc2810acef6a812844641175b88e4`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- General reviewed baseline before this targeted re-review:
  `decbf641b18f1982b3475c037e7c5c554471574f`
- Live upstream head queried: `20d29fc6bc2fc2b58d1fff8d8e0503b9ba7f41d8`
- Query time: `2026-10-01T23:11:20Z`
- Local implementation commit: `bb6bba0d70`

This is a targeted re-review of the upstream Mermaid history from the first
Agent UI integration through the current `merman` renderer. It supersedes the
older Mermaid `C` decisions that were based on ZZZ lacking the
`mermaid_render` architecture. It does not review every upstream commit after
`decbf641b1`, so the general reviewed baseline remains unchanged.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                              |
| ---------- | ----- | ------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2301e61d2a | B     | bb6bba0d70   | Ported theme invalidation, code/preview controls, fenced-block validation, and ACP rendering integration. Native Zed agent prompt changes remain absent. |
| 29db565437 | A     | 46bbd198ec   | Already equivalent: ZZZ resolves its virtual UI fonts before Mermaid text measurement.                                                                   |
| 0042fb5850 | A     | 45496f413b   | Already equivalent: code blocks retain the upstream wrap/unwrap behavior.                                                                                |
| 63f725e8d6 | B     | bb6bba0d70   | Replaced the vendored `mermaid-rs-renderer` with the upstream `mermaid_render` crate and `merman` pipeline.                                              |
| f0ed342c19 | A     | b40e6516ed   | Already equivalent: frontmatter metadata rendering is present locally.                                                                                   |
| 4e949a1cc3 | B     | bb6bba0d70   | Absorbed the cyclic/deep subgraph safety behavior through the final pinned `merman` release and regression tests.                                        |
| 4eab06968b | B     | bb6bba0d70   | Absorbed the renderer assertion fix through the final `merman` implementation.                                                                           |
| 6d72acdb99 | B     | bb6bba0d70   | Ported the raster-safe SVG pipeline and retained only its final form.                                                                                    |
| a5f0dc322d | B     | bb6bba0d70   | Ported Zed-specific SVG postprocessing, theme CSS, accent colors, and compatibility fixes.                                                               |
| 4ab04b3245 | B     | bb6bba0d70   | Ported the Mermaid Preview/Code button controls and adapted their text to ZZZ i18n.                                                                      |
| 49eb6b2de7 | B     | bb6bba0d70   | Ported long-label wrapping and drawable-SVG regression coverage.                                                                                         |
| c88c83d619 | C     | --           | Intermediate `merman` dependency and lockfile-only revision update; superseded by the final crates.io `0.8.0-alpha.5` dependency.                        |
| f791aa57d7 | B     | bb6bba0d70   | Retained the mixed-font panic regression and SVG compatibility behavior while keeping ZZZ's newer `resvg`/`usvg` 0.48 stack.                             |
| c49a29f461 | B     | bb6bba0d70   | Ported only the stable per-Markdown Mermaid element IDs. The native-agent sandbox rewrite is omitted.                                                    |
| fbd911ed3e | A     | b5c3ee7a8b   | Already equivalent: GPUI caps SVG pixmaps at 8192 pixels.                                                                                                |
| fd8f0cdfd7 | B     | bb6bba0d70   | Ported intrinsic-size rendering and horizontal scrolling for wide diagrams.                                                                              |
| 00cba838ad | B     | bb6bba0d70   | Ported parsed-SVG caching, debounced rerasterization, zoom controls, stable scroll state, GPU image cleanup, and ACP list anchoring.                     |
| c305d68c01 | C     | --           | Intermediate dependency and lockfile-only bump to `merman` 0.7; superseded by `ec18126b1d`.                                                              |
| 4bd1993783 | B     | bb6bba0d70   | Ported triple-tilde Mermaid fences and closed-fence detection.                                                                                           |
| 03c9c4e707 | B     | bb6bba0d70   | Ported exact-size `ParsedSvg` rasterization and GPUI regression tests.                                                                                   |
| 282f47a544 | C     | --           | Repository-wide cargo-shear migration is unrelated product infrastructure; the new crate carries both local and upstream ignore metadata.                |
| ec18126b1d | B     | bb6bba0d70   | Pinned the current upstream `merman = 0.8.0-alpha.5` stack, including matching alpha.5 transitive crates in `Cargo.lock`.                                |
| 6b5e15ed46 | C     | --           | Broad dependency and lockfile churn with no independent Mermaid behavior required by ZZZ.                                                                |
| 07df438626 | C     | --           | Upstream-only GPUI tracing cleanup; the Mermaid migration has no dependency on this change.                                                              |

Totals: four `A`, fifteen `B`, five `C`.

## Applied work

`bb6bba0d70` is a consolidated B port because the final upstream Mermaid files
span a dependency migration and several follow-up fixes that cannot compile
independently against ZZZ's newer GPUI and Markdown trees. The commit carries
an `Upstream:` trailer for each retained B-class change.

The port:

- adds the workspace `mermaid_render` crate and removes the 51,000-line
  vendored `mermaid-rs-renderer` tree;
- renders with `merman`, runs its resvg-safe SVG pipeline, then applies Zed's
  XML/CSS theme and accent-color postprocessing;
- maps the active ZZZ theme, font, status colors, and player palette into each
  render and invalidates cached diagrams on theme changes;
- caches parsed SVG trees separately from raster images, rerasterizes after a
  debounced zoom, and explicitly releases replaced GPU images;
- adds Preview/Code tabs, copy controls, Ctrl/Cmd-scroll zoom, reset controls,
  stable horizontal scrolling, and ACP tail-follow anchoring;
- supports closed backtick and tilde fences and rejects unsupported diagram
  types before rendering;
- localizes all new Markdown controls in English and Simplified Chinese.

The native Zed agent prompt edits are absent because ZZZ receives agent
instructions through ACP. Unrelated sandbox, telemetry, repository-tooling,
and broad dependency updates were not imported.

## Verification

| Check                                                                                         | Result                                                                                                                                                                          |
| --------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Live `git ls-remote` and `git fetch --no-tags` to `FETCH_HEAD`                                | PASS                                                                                                                                                                            |
| `git diff --check`                                                                            | PASS                                                                                                                                                                            |
| `cargo fmt --all -- --check`                                                                  | PASS                                                                                                                                                                            |
| `cargo check --locked -p mermaid_render -p markdown -p markdown_preview -p agent_ui`          | PASS                                                                                                                                                                            |
| `cargo test --locked -p mermaid_render`                                                       | PASS, 14 tests                                                                                                                                                                  |
| `cargo test --locked -p markdown mermaid`                                                     | PASS, 18 tests                                                                                                                                                                  |
| `cargo test -p markdown parser::tests::test_`                                                 | PASS, 31 tests                                                                                                                                                                  |
| GPUI ParsedSvg, child-scroll, and pause-following-tail tests                                  | PASS, 5 tests                                                                                                                                                                   |
| `cargo test -p markdown`                                                                      | BASELINE FAIL: 191 passed; four unrelated clipboard/table tests failed. `test_center_aligned_table_header_selection_uses_visual_offset` reproduces from clean `main` in `/tmp`. |
| Three targeted `cargo clippy` passes used by `./script/clippy`                                | PASS                                                                                                                                                                            |
| `./script/clippy -p mermaid_render -p markdown -p gpui -p agent_ui` auxiliary `cargo machete` | BASELINE FAIL: existing unused dependencies in `vendor/font-kit`, `vendor/wgpu/naga-test`, and `vendor/windows-capture`; no Mermaid dependency is reported.                     |
| `./script/check-philosophy`                                                                   | PASS                                                                                                                                                                            |
| English / Simplified Chinese recursive locale key-set comparison                              | PASS, 3495 keys                                                                                                                                                                 |
| macOS / Windows runtime checks                                                                | NOT RUN                                                                                                                                                                         |
