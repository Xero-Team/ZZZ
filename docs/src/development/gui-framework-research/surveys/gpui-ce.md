# Static survey: gpui-ce

- Checkout: `/home/begonia/Documents/Github/XeroTeam/ZZZ/.tmp/ui_ref/gpui-ce`
- HEAD: `175ef66578817bf96b2e26b0cd568dd4f4793529`
- Tracked files: 464

## Top-level subsystems

- `crates`: 412 files
- `assets`: 11 files
- `docs`: 8 files
- `.github`: 5 files
- `scripts`: 5 files
- `tooling`: 5 files
- `.cargo`: 2 files
- `.gitattributes`: 1 files
- `.gitignore`: 1 files
- `CONTRIBUTING.md`: 1 files
- `Cargo.lock`: 1 files
- `Cargo.toml`: 1 files
- `LICENSE.md`: 1 files
- `README.md`: 1 files
- `SETUP.md`: 1 files
- `audit.toml`: 1 files
- `clippy.toml`: 1 files
- `deny.toml`: 1 files
- `flake.lock`: 1 files
- `flake.nix`: 1 files
- `justfile`: 1 files
- `rheo.toml`: 1 files
- `typos.toml`: 1 files

## Manifests

- `Cargo.toml`
- `crates/gpui/Cargo.toml`
- `crates/gpui_apple/Cargo.toml`
- `crates/gpui_ce_util/Cargo.toml`
- `crates/gpui_collections/Cargo.toml`
- `crates/gpui_derive_refineable/Cargo.toml`
- `crates/gpui_elements/Cargo.toml`
- `crates/gpui_linux/Cargo.toml`
- `crates/gpui_macos/Cargo.toml`
- `crates/gpui_macros/Cargo.toml`
- `crates/gpui_media/Cargo.toml`
- `crates/gpui_path/Cargo.toml`
- `crates/gpui_platform/Cargo.toml`
- `crates/gpui_refineable/Cargo.toml`
- `crates/gpui_render/Cargo.toml`
- `crates/gpui_scheduler/Cargo.toml`
- `crates/gpui_shared_string/Cargo.toml`
- `crates/gpui_sum_tree/Cargo.toml`
- `crates/gpui_tokio/Cargo.toml`
- `crates/gpui_web/Cargo.toml`
- `crates/gpui_web/examples/hello_web/Cargo.toml`
- `crates/gpui_wgpu/Cargo.toml`
- `crates/gpui_windows/Cargo.toml`
- `crates/gpui_zed_util/Cargo.toml`
- `tooling/perf/Cargo.toml`

## Candidate entrypoints

- `crates/gpui_ce_util/src/lib.rs`
- `crates/gpui_elements/src/lib.rs`
- `crates/gpui_web/examples/hello_web/main.rs`
- `tooling/perf/src/lib.rs`
- `tooling/perf/src/main.rs`

## Tests, benchmarks, and evals

- `crates/gpui/src/app/test_app.rs`
- `crates/gpui/src/app/test_context.rs`
- `crates/gpui/src/platform/test/dispatcher.rs`
- `crates/gpui/src/platform/test/display.rs`
- `crates/gpui/src/platform/test/platform.rs`
- `crates/gpui/src/platform/test/window.rs`
- `crates/gpui/tests/action_macros.rs`
- `crates/gpui/tests/derive_context.rs`
- `crates/gpui/tests/derive_element_traits.rs`
- `crates/gpui/tests/derive_inspector_reflection.rs`
- `crates/gpui/tests/render_macro.rs`
- `crates/gpui/tests/renderer_source_audit.rs`
- `crates/gpui_scheduler/src/test_scheduler.rs`
- `crates/gpui_wgpu/tests/headless_primitives.rs`
- `crates/gpui_zed_util/src/test/assertions.rs`
- `crates/gpui_zed_util/src/test/marked_text.rs`

## Architecture and policy documents

- `CONTRIBUTING.md`
- `README.md`
- `crates/gpui/README.md`
- `scripts/sync-upstream/README.md`

## Declared dependencies

- `accesskit`
- `accesskit_consumer`
- `anyhow`
- `async-channel`
- `async-task`
- `async_zip`
- `backtrace`
- `bindgen`
- `bitflags`
- `bytemuck`
- `chrono`
- `collections`
- `cosmic-text`
- `criterion`
- `ctor`
- `derive_more`
- `derive_refineable`
- `derive_setters`
- `dunce`
- `embed-resource`
- `env_logger`
- `etagere`
- `flume`
- `font-kit`
- `futures`
- `futures-concurrency`
- `futures-lite`
- `globset`
- `gpui`
- `gpui_macros`
- `gpui_platform`
- `gpui_render`
- `gpui_shared_string`
- `gpui_util`
- `hdrhistogram`
- `heapless`
- `heck`
- `http`
- `image`
- `indexmap`
- `inventory`
- `itertools`
- `log`
- `lyon`
- `naga`
- `naga-old`
- `num_cpus`
- `palette`
- `parking`
- `parking_lot`
- `path`
- `percent-encoding`
- `pin-project`
- `pollster`
- `postage`
- `pretty_assertions`
- `proc-macro2`
- `profiling`
- `proptest`
- `quote`
- `rand`
- `raw-window-handle`
- `rayon`
- `refineable`
- `regex`
- `resvg`
- `rust-embed`
- `rustc-hash`
- `scheduler`
- `schemars`
- `seahash`
- `serde`
- `serde_json`
- `serde_json_lenient`
- `shlex`
- `skrifa`
- `slotmap`
- `smallvec`
- `smol`
- `smol_str`
- … 23 more

## Architecture keyword hints

- **agent**: `.github/workflows/prerelease.yml`, `clippy.toml`, `crates/gpui_macros/src/derive_inspector_reflection.rs`, `crates/gpui_refineable/src/refineable.rs`, `crates/gpui_web/src/events.rs`, `crates/gpui_web/src/platform.rs`, `crates/gpui_windows/src/window.rs`, `crates/gpui_zed_util/src/process.rs`, `crates/gpui_zed_util/src/shell_builder.rs`, `scripts/sync-upstream/sync_upstream.py`
- **benchmark**: `crates/gpui/Cargo.toml`, `crates/gpui/src/app/bench_context.rs`, `crates/gpui/src/gpui.rs`, `crates/gpui/src/platform/test/platform.rs`, `crates/gpui/src/platform/threaded_dispatcher.rs`, `crates/gpui/src/window.rs`, `crates/gpui_apple/src/metal_renderer.rs`, `crates/gpui_macros/src/bench.rs`, `crates/gpui_macros/src/gpui_macros.rs`, `crates/gpui_wgpu/benches/layout_line.rs`, `crates/gpui_wgpu/benches/renderer.rs`, `crates/gpui_wgpu/src/wgpu_renderer/headless.rs`
- **browser**: `crates/gpui/src/_accessibility.rs`, `crates/gpui/src/app.rs`, `crates/gpui/src/elements/div.rs`, `crates/gpui/src/elements/surface.rs`, `crates/gpui/src/geometry.rs`, `crates/gpui/src/gestures.rs`, `crates/gpui/src/platform.rs`, `crates/gpui_linux/src/linux/wayland/client.rs`, `crates/gpui_scheduler/Cargo.toml`, `crates/gpui_web/src/dispatcher.rs`, `crates/gpui_web/src/display.rs`, `crates/gpui_web/src/events.rs`
- **cache**: `.cargo/config.toml`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `crates/gpui/examples/legacy/image_loading.rs`, `crates/gpui/examples/view_example/example_editor.rs`, `crates/gpui/examples/view_example/example_input.rs`, `crates/gpui/examples/view_example/example_text_area.rs`, `crates/gpui/src/app.rs`, `crates/gpui/src/app/bench_context.rs`, `crates/gpui/src/assets.rs`, `crates/gpui/src/bounds_tree.rs`, `crates/gpui/src/elements/deferred.rs`
- **embed**: `crates/gpui/Cargo.toml`, `crates/gpui/build.rs`, `crates/gpui/examples/view_example/example_editor.rs`, `crates/gpui/src/app.rs`, `crates/gpui/src/asset_cache.rs`, `crates/gpui/src/assets.rs`, `crates/gpui/src/elements/img.rs`, `crates/gpui/src/platform.rs`, `crates/gpui/src/view.rs`, `crates/gpui/src/window.rs`, `crates/gpui_macos/src/text_system.rs`, `crates/gpui_render/build.rs`
- **evaluation**: `crates/gpui/src/style_transitions.rs`, `crates/gpui/src/transition.rs`, `crates/gpui_elements/src/editable_text/state.rs`
- **graph**: `Cargo.toml`, `SETUP.md`, `crates/gpui/Cargo.toml`, `crates/gpui/examples/learn/text.rs`, `crates/gpui/examples/view_example/example_editor.rs`, `crates/gpui/src/asset_cache.rs`, `crates/gpui/src/elements/surface.rs`, `crates/gpui/src/geometry.rs`, `crates/gpui/src/platform.rs`, `crates/gpui/src/platform/screen_capture/windows.rs`, `crates/gpui/src/platform/windows_screen_capture.rs`, `crates/gpui/src/style.rs`
- **index**: `Cargo.toml`, `README.md`, `clippy.toml`, `crates/gpui/examples/learn/custom_drawing.rs`, `crates/gpui/examples/learn/interactive_elements.rs`, `crates/gpui/examples/learn/motion_showcase.rs`, `crates/gpui/examples/learn/styling.rs`, `crates/gpui/examples/learn/uniform_list.rs`, `crates/gpui/examples/legacy/focus_visible.rs`, `crates/gpui/examples/legacy/tab_stop.rs`, `crates/gpui/examples/view_example/example_editor.rs`, `crates/gpui/src/_accessibility.rs`
- **mcp**: `crates/gpui_zed_util/src/process.rs`
- **plugin**: `crates/gpui_zed_util/src/util.rs`
- **provider**: `crates/gpui/src/elements/div.rs`, `crates/gpui/src/elements/image_cache.rs`, `crates/gpui/src/svg_renderer.rs`, `crates/gpui_macos/src/text_system.rs`, `crates/gpui_macos/src/window.rs`, `crates/gpui_windows/src/events.rs`, `crates/gpui_windows/src/platform.rs`, `crates/gpui_windows/src/vsync.rs`, `crates/gpui_windows/src/window.rs`
- **queue**: `.github/workflows/prerelease.yml`, `crates/gpui/src/app/async_context.rs`, `crates/gpui/src/app/bench_context.rs`, `crates/gpui/src/executor.rs`, `crates/gpui/src/gpui.rs`, `crates/gpui/src/platform.rs`, `crates/gpui/src/platform/test/platform.rs`, `crates/gpui/src/platform/threaded_dispatcher.rs`, `crates/gpui/src/platform_scheduler.rs`, `crates/gpui/src/profiler/journal.rs`, `crates/gpui/src/queue.rs`, `crates/gpui/src/window.rs`
- **rank**: `crates/gpui/src/keymap.rs`, `crates/gpui_render/src/shaders/corner_smoothing.rs`, `crates/gpui_wgpu/src/wgpu_context.rs`
- **retriev**: `crates/gpui/src/app/test_context.rs`, `crates/gpui/src/elements/div.rs`, `crates/gpui/src/elements/text.rs`, `crates/gpui/src/keymap/binding.rs`, `crates/gpui/src/platform.rs`, `crates/gpui/src/text_system/line_layout.rs`, `crates/gpui_apple/src/metal_renderer.rs`, `crates/gpui_linux/src/linux/x11/client.rs`, `crates/gpui_macos/src/text_system.rs`, `crates/gpui_wgpu/src/cosmic_text_system.rs`, `crates/gpui_windows/src/events.rs`, `crates/gpui_windows/src/vsync.rs`
- **sandbox**: `crates/gpui_zed_util/src/util.rs`
- **search**: `Cargo.toml`, `crates/gpui/src/app.rs`, `crates/gpui/src/bounds_tree.rs`, `crates/gpui/src/elements/div.rs`, `crates/gpui/src/key_dispatch.rs`, `crates/gpui/src/platform.rs`, `crates/gpui/src/window.rs`, `crates/gpui_linux/src/linux/platform.rs`, `crates/gpui_macos/src/text_system.rs`, `crates/gpui_path/src/rel_path.rs`, `crates/gpui_sum_tree/src/cursor.rs`, `crates/gpui_web/examples/hello_web/main.rs`
- **symbol**: `.cargo/config.toml`, `crates/gpui/examples/bench/data_table.rs`, `crates/gpui/examples/learn/text.rs`, `crates/gpui/src/app/entity_map.rs`, `crates/gpui/src/svg_renderer.rs`, `crates/gpui_linux/src/linux/platform.rs`, `crates/gpui_macos/src/open_type.rs`, `crates/gpui_macos/src/text_system.rs`, `crates/gpui_scheduler/src/test_scheduler.rs`, `crates/gpui_wgpu/src/cosmic_text_system.rs`, `crates/gpui_windows/src/direct_write.rs`, `crates/gpui_zed_util/src/paths.rs`
- **telemetry**: `crates/gpui/src/interactive.rs`, `crates/gpui/src/profiler/hang.rs`
- **vector**: `crates/gpui/examples/learn/custom_drawing.rs`, `crates/gpui/src/elements/deferred.rs`, `crates/gpui/src/elements/div.rs`, `crates/gpui/src/gestures.rs`, `crates/gpui/src/path_builder.rs`, `crates/gpui/src/scene.rs`, `crates/gpui/src/scene/abi.rs`, `crates/gpui/src/window.rs`, `crates/gpui_collections/src/vecmap.rs`, `crates/gpui_macos/src/text_system.rs`, `crates/gpui_render/build.rs`, `crates/gpui_render/src/shaders/tests.rs`

## Interpretation limit

This survey identifies candidate files. It does not establish behavior; read and cite the implementation and tests before making claims.
