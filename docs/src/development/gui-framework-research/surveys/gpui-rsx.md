# Static survey: gpui-rsx

- Checkout: `/home/begonia/Documents/Github/XeroTeam/ZZZ/.tmp/ui_ref/gpui-rsx`
- HEAD: `8e0751e9361c08af1ceec702be10b50fbf4e412f`
- Tracked files: 143

## Top-level subsystems

- `tests`: 59 files
- `docs`: 35 files
- `demo`: 21 files
- `src`: 9 files
- `.github`: 4 files
- `.gitignore`: 1 files
- `ARCHITECTURE.md`: 1 files
- `ARCHITECTURE_CN.md`: 1 files
- `CHANGELOG.md`: 1 files
- `CHANGELOG_CN.md`: 1 files
- `CONTRIBUTING.md`: 1 files
- `Cargo.toml`: 1 files
- `LICENSE`: 1 files
- `README.md`: 1 files
- `README_CN.md`: 1 files
- `benches`: 1 files
- `codecov.yml`: 1 files
- `coverage.sh`: 1 files
- `scripts`: 1 files
- `todo`: 1 files

## Manifests

- `Cargo.toml`
- `demo/Cargo.toml`
- `docs/package.json`

## Candidate entrypoints

- `demo/src/bin/incident_console/main.rs`
- `src/lib.rs`

## Tests, benchmarks, and evals

- `tests/common/capture.rs`
- `tests/common/mod.rs`
- `tests/common/types.rs`
- `tests/coverage_tests.rs`
- `tests/diagnostic_tests.rs`
- `tests/fixtures/gpui_stateful_methods_e973593.txt`
- `tests/gpui_api_snapshot.rs`
- `tests/macro_tests.rs`
- `tests/pass/font_weight_mapping.rs`
- `tests/pass/latest_gpui_loop_contracts.rs`
- `tests/pass/latest_gpui_stateful_contract.rs`
- `tests/pass/state_class_and_a11y.rs`
- `tests/pass/strict_permissive_modes.rs`
- `tests/ui/for_aria_description_missing_key.rs`
- `tests/ui/for_aria_description_missing_key.stderr`
- `tests/ui/for_missing_key.rs`
- `tests/ui/for_missing_key.stderr`
- `tests/ui/for_restrict_scroll_missing_key.rs`
- `tests/ui/for_restrict_scroll_missing_key.stderr`
- `tests/ui/invalid_arbitrary_color.rs`
- `tests/ui/invalid_arbitrary_color.stderr`
- `tests/ui/invalid_arbitrary_length.rs`
- `tests/ui/invalid_arbitrary_length.stderr`
- `tests/ui/invalid_arbitrary_length_nan.rs`
- `tests/ui/invalid_arbitrary_length_nan.stderr`
- `tests/ui/invalid_fraction.rs`
- `tests/ui/invalid_fraction.stderr`
- `tests/ui/invalid_fraction_negative.rs`
- `tests/ui/invalid_fraction_negative.stderr`
- `tests/ui/invalid_opacity.rs`
- `tests/ui/invalid_opacity.stderr`
- `tests/ui/invalid_spacing_percent.rs`
- `tests/ui/invalid_spacing_percent.stderr`
- `tests/ui/path_tag_mismatch.rs`
- `tests/ui/path_tag_mismatch.stderr`
- `tests/ui/state_class_dynamic.rs`
- `tests/ui/state_class_dynamic.stderr`
- `tests/ui/state_class_element_only.rs`
- `tests/ui/state_class_element_only.stderr`
- `tests/ui/state_class_stateful.rs`
- `tests/ui/state_class_stateful.stderr`
- `tests/ui/strict_unknown_class.rs`
- `tests/ui/strict_unknown_class.stderr`
- `tests/ui/tag_mismatch.rs`
- `tests/ui/tag_mismatch.stderr`
- `tests/ui/unsupported_font_weight.rs`
- `tests/ui/unsupported_font_weight.stderr`
- `tests/ui/unsupported_group_drag_over.rs`
- `tests/ui/unsupported_group_drag_over.stderr`
- `tests/ui/unsupported_whitespace_attr.rs`
- `tests/ui/unsupported_whitespace_attr.stderr`
- `tests/ui/when_class_dynamic.rs`
- `tests/ui/when_class_dynamic.stderr`
- `tests/ui/when_class_stateful.rs`
- `tests/ui/when_class_stateful.stderr`
- `tests/ui/when_class_wrong_count.rs`
- `tests/ui/when_class_wrong_count.stderr`
- `tests/ui/when_wrong_count.rs`
- `tests/ui/when_wrong_count.stderr`

## Architecture and policy documents

- `ARCHITECTURE.md`
- `ARCHITECTURE_CN.md`
- `CONTRIBUTING.md`
- `README.md`
- `README_CN.md`
- `demo/README.md`

## Declared dependencies

- `@astrojs/check`
- `@astrojs/starlight`
- `astro`
- `criterion`
- `gpui`
- `gpui-component`
- `gpui-rsx`
- `gpui_platform`
- `proc-macro2`
- `quote`
- `sharp`
- `syn`
- `trybuild`
- `typescript`

## Architecture keyword hints

- **benchmark**: `CHANGELOG.md`, `CHANGELOG_CN.md`, `benches/class_performance.rs`, `docs/src/content/docs/guides/performance.md`, `docs/src/content/docs/reference/release-checklist.md`, `docs/src/content/docs/zh-cn/guides/performance.md`, `docs/src/content/docs/zh-cn/reference/release-checklist.md`, `todo/UPGRADE_TO_LATEST_GPUI-0825.md`
- **browser**: `CONTRIBUTING.md`, `docs/src/content/docs/index.md`, `docs/src/content/docs/usage/syntax.md`
- **cache**: `.github/workflows/ci.yml`, `.github/workflows/gpui-compatibility.yml`, `ARCHITECTURE.md`, `CHANGELOG.md`, `CHANGELOG_CN.md`, `README.md`, `demo/src/bin/incident_console/sample_data.rs`, `src/codegen/runtime.rs`, `src/codegen/tables.rs`, `tests/common/mod.rs`, `tests/macro_tests.rs`, `todo/UPGRADE_TO_LATEST_GPUI-0825.md`
- **evaluation**: `CHANGELOG.md`, `tests/macro_tests.rs`
- **graph**: `.github/dependabot.yml`, `CHANGELOG.md`, `README.md`, `README_CN.md`, `docs/src/content/docs/compatibility.md`, `docs/src/content/docs/guides/migration.md`, `docs/src/content/docs/guides/performance.md`, `docs/src/content/docs/reference/api.md`, `docs/src/content/docs/usage/class.md`, `src/lib.rs`, `tests/macro_tests.rs`
- **index**: `.github/workflows/ci.yml`, `CHANGELOG.md`, `CHANGELOG_CN.md`, `README_CN.md`, `demo/src/bin/incident_console/sample_data.rs`, `docs/src/content/docs/reference/api.md`, `docs/src/content/docs/usage/syntax.md`, `docs/src/content/docs/zh-cn/reference/api.md`, `docs/src/content/docs/zh-cn/usage/syntax.md`, `src/codegen/runtime.rs`, `src/codegen/tables.rs`, `src/parser.rs`
- **queue**: `demo/src/bin/incident_console/details.rs`, `demo/src/bin/incident_console/main.rs`, `demo/src/bin/incident_console/model.rs`, `demo/src/bin/incident_console/sample_data.rs`, `demo/src/bin/incident_console/tests.rs`, `demo/src/bin/incident_console/view.rs`, `demo/src/bin/project_dashboard.rs`, `docs/src/content/docs/examples/incident-console.md`, `docs/src/content/docs/zh-cn/examples/incident-console.md`
- **search**: `demo/src/bin/incident_console/sample_data.rs`

## Interpretation limit

This survey identifies candidate files. It does not establish behavior; read and cite the implementation and tests before making claims.
