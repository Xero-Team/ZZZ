# Static survey: adabraka-ui

- Checkout: `/home/begonia/Documents/Github/XeroTeam/ZZZ/.tmp/ui_ref/adabraka-ui`
- HEAD: `e158684b23d9cb043fed3989ca252212046dabca`
- Tracked files: 3634

## Top-level subsystems

- `assets`: 3287 files
- `src`: 175 files
- `examples`: 144 files
- `docs`: 5 files
- `.github`: 4 files
- `.claude-tasks.md`: 1 files
- `.gitignore`: 1 files
- `CHANGELOG.md`: 1 files
- `CLAUDE.md`: 1 files
- `COMPONENT_REVIEW.md`: 1 files
- `CONTRIBUTING.md`: 1 files
- `Cargo.lock`: 1 files
- `Cargo.toml`: 1 files
- `GAP_ANALYSIS.md`: 1 files
- `LAYOUT_GUIDE.md`: 1 files
- `LAYOUT_QUICK_REFERENCE.md`: 1 files
- `LICENSE`: 1 files
- `LICENSE-MIT`: 1 files
- `README.md`: 1 files
- `RELEASE_NOTES_v0.1.0.md`: 1 files
- `RELEASE_NOTES_v0.2.0.md`: 1 files
- `RELEASE_NOTES_v0.2.2.md`: 1 files
- `ROADMAP.md`: 1 files
- `shiori-go-test.png`: 1 files

## Manifests

- `Cargo.toml`
- `assets/icons/package.json`

## Candidate entrypoints

- `src/lib.rs`

## Tests, benchmarks, and evals

- `examples/test_element_id.rs`
- `examples/test_extra_fields.rs`
- `examples/test_horizontal_scroll.rs`
- `examples/test_real_scrollcontainer.rs`
- `examples/test_scroll_container.rs`
- `examples/test_simple_scroll.rs`

## Architecture and policy documents

- `CONTRIBUTING.md`
- `README.md`
- `assets/fonts/README.md`

## Declared dependencies

- `bytes`
- `futures`
- `gpui`
- `html5ever`
- `isahc`
- `markup5ever_rcdom`
- `once_cell`
- `pulldown-cmark`
- `qrcode`
- `regex`
- `rodio`
- `ropey`
- `smallvec`
- `smol`
- `tree-sitter`
- `tree-sitter-bash`
- `tree-sitter-c`
- `tree-sitter-cpp`
- `tree-sitter-css`
- `tree-sitter-go`
- `tree-sitter-html`
- `tree-sitter-java`
- `tree-sitter-javascript`
- `tree-sitter-json`
- `tree-sitter-lua`
- `tree-sitter-md`
- `tree-sitter-ocaml`
- `tree-sitter-php`
- `tree-sitter-python`
- `tree-sitter-ruby`
- `tree-sitter-rust`
- `tree-sitter-scala`
- `tree-sitter-sequel`
- `tree-sitter-toml-ng`
- `tree-sitter-typescript`
- `tree-sitter-yaml`
- `tree-sitter-zig`
- `unicode-segmentation`

## Architecture keyword hints

- **agent**: `assets/icons/hat-glasses.json`, `src/display/html.rs`, `src/http.rs`, `src/lib.rs`, `src/prelude.rs`, `src/theme/theme.rs`, `src/theme/tokens.rs`
- **benchmark**: `ROADMAP.md`, `examples/data_table_demo.rs`
- **browser**: `ROADMAP.md`, `assets/icons/chromium.json`, `assets/icons/compass.json`, `assets/icons/earth-lock.json`, `assets/icons/earth.json`, `assets/icons/file-search-2.json`, `assets/icons/file-search.json`, `assets/icons/folder-search-2.json`, `assets/icons/folder-search.json`, `assets/icons/folder-tree.json`, `assets/icons/globe-lock.json`, `assets/icons/globe.json`
- **cache**: `assets/icons/database-zap.json`, `src/animations.rs`, `src/components/editor.rs`, `src/display/data_table.rs`, `src/fonts.rs`
- **embed**: `src/components/editor.rs`, `src/fonts.rs`
- **graph**: `CHANGELOG.md`, `GAP_ANALYSIS.md`, `README.md`, `RELEASE_NOTES_v0.1.0.md`, `ROADMAP.md`, `assets/icons/album.json`, `assets/icons/ampersand.json`, `assets/icons/aperture.json`, `assets/icons/audio-lines.json`, `assets/icons/backpack.json`, `assets/icons/binoculars.json`, `assets/icons/blend.json`
- **index**: `CHANGELOG.md`, `COMPONENT_REVIEW.md`, `GAP_ANALYSIS.md`, `LAYOUT_GUIDE.md`, `LAYOUT_QUICK_REFERENCE.md`, `README.md`, `RELEASE_NOTES_v0.2.2.md`, `ROADMAP.md`, `assets/icons/archive-restore.json`, `assets/icons/archive-x.json`, `assets/icons/archive.json`, `assets/icons/book-marked.json`
- **plugin**: `assets/icons/blocks.json`, `assets/icons/toy-brick.json`, `examples/tabs_demo.rs`
- **provider**: `assets/icons/card-sim.json`, `examples/virtual_list_demo.rs`, `src/virtual_list.rs`
- **queue**: `assets/icons/columns-2.json`, `assets/icons/columns-3.json`, `assets/icons/columns-4.json`, `assets/icons/list-end.json`, `assets/icons/list-minus.json`, `assets/icons/list-music.json`, `assets/icons/list-ordered.json`, `assets/icons/list-start.json`, `assets/icons/list-x.json`, `assets/icons/logs.json`, `assets/icons/rows-2.json`, `assets/icons/rows-3.json`
- **rank**: `assets/icons/shield-half.json`, `examples/avatar_styled_demo.rs`, `examples/combobox_demo.rs`, `examples/mention_input_demo.rs`
- **retriev**: `assets/icons/squirrel.json`
- **search**: `.github/ISSUE_TEMPLATE/bug_report.md`, `.github/ISSUE_TEMPLATE/feature_request.md`, `CHANGELOG.md`, `GAP_ANALYSIS.md`, `README.md`, `RELEASE_NOTES_v0.1.0.md`, `RELEASE_NOTES_v0.2.0.md`, `ROADMAP.md`, `assets/icons/binoculars.json`, `assets/icons/book-a.json`, `assets/icons/book-alert.json`, `assets/icons/book-audio.json`
- **symbol**: `assets/icons/component.json`, `assets/icons/file-symlink.json`, `assets/icons/folder-symlink.json`, `assets/icons/omega.json`, `assets/icons/pi.json`, `assets/icons/pilcrow-left.json`, `assets/icons/pilcrow-right.json`, `assets/icons/pilcrow.json`, `assets/icons/radical.json`, `assets/icons/section.json`, `assets/icons/square-pi.json`, `assets/icons/square-pilcrow.json`
- **tree_sitter**: `src/components/editor.rs`
- **vector**: `assets/icons/pen-tool.json`, `assets/icons/spline-pointer.json`, `assets/icons/squares-exclude.json`, `assets/icons/squares-intersect.json`, `assets/icons/squares-subtract.json`, `assets/icons/squares-unite.json`

## Interpretation limit

This survey identifies candidate files. It does not establish behavior; read and cite the implementation and tests before making claims.
