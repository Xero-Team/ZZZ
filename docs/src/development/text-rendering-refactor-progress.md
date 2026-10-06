---
title: Text Rendering Refactor Progress
description: Execution ledger for the staged GPUI text rendering and sprite atlas refactor.
---

# GPUI 文本渲染重构进度账本

本账本按
[GPUI 文本渲染完整重构执行计划](./text-rendering-research/refactoring-plan.md)
记录阶段 0–9 的基线、实验、验证、提交和平台欠账。原始 benchmark、pixel
output、trace 和临时 fixture 保存在 `.tmp/text-rendering-refactor/`。

## 执行状态

| 项目          | 值                                         |
| ------------- | ------------------------------------------ |
| 工作分支      | `refactor/gpui-text-rendering`             |
| 计划基线      | `a3a0f9734069b543f3fe1e0bdd77a37fbd1b2b31` |
| 执行基线      | `a3a0f9734069b543f3fe1e0bdd77a37fbd1b2b31` |
| 当前阶段      | 阶段 2 进行中                              |
| Goal 状态     | `ACTIVE`                                   |
| 固定随机 seed | `0x5A5A_5445_5854_2026`                    |

执行开始时 HEAD 与计划基线相同，checkout 位于 `main`。工作树已有用户修改：
`docs/src/SUMMARY.md`、`docs/src/development/text-layout.md` 和未跟踪的
`docs/src/development/text-rendering-research/`。这些内容原样保留；在任何代码修改前
创建并切换到 `refactor/gpui-text-rendering`，没有 reset、clean、覆盖或丢弃修改。

## 环境基线

| 项目             | 值                                                            | 状态   |
| ---------------- | ------------------------------------------------------------- | ------ |
| Host             | Fedora Linux `7.3.0-0.rc4.260925g165768bb7026.42.fc46.x86_64` | `PASS` |
| Rust             | `rustc 1.99.0 (b940084d7 2026-09-28)`                         | `PASS` |
| Cargo            | `cargo 1.99.0 (5f94df478 2026-08-27)`                         | `PASS` |
| Target           | `x86_64-unknown-linux-gnu`                                    | `PASS` |
| Session          | Wayland，`DISPLAY=:0`，`WAYLAND_DISPLAY=wayland-0`            | `PASS` |
| GPU              | AMD Radeon 8060S Graphics，RADV Mesa 26.2.3                   | `PASS` |
| Software adapter | llvmpipe，Mesa 26.2.3，LLVM 23.1.0                            | `PASS` |
| Vulkan           | Instance 1.4.357；RADV 与 llvmpipe 均可枚举                   | `PASS` |
| Fonts            | Noto Sans Mono、Noto Sans、Noto Serif                         | `PASS` |

## 阶段记录

### 阶段 0：固定 baseline 和可观测性

状态：`COMPLETE`

已完成：

- 完整读取仓库规则、文档规则、研究报告、执行合同、证据和实验表，以及执行合同引用的
  GPUI ownership/refactor 文档。
- 确认原始实现仍把 atlas domain 放在 `platform.rs`，且 WGPU、Metal、DirectX
  分别持有 allocator、free list 和 live-key policy。
- 确认现有真实 headless renderer 可在当前主机分别使用 RADV 与 llvmpipe。
- 固定执行分支、计划基线、主机、toolchain、GPU、driver 和字体环境。
- 增加 `AtlasSnapshot`，明确 gauge 与累计 counter 的单位，并覆盖 page count、resident
  page bytes、entry、hit/miss、allocation、pending/upload、removal，以及后续阶段启用的
  retirement、eviction、compaction、working-set 和 budget-pressure 字段。
- WGPU、Metal、DirectX 从真实 texture page 和 pixel format 计算 resident bytes；WGPU
  区分 pending upload 与已提交 upload，Metal/DirectX 记录即时 upload。
- snapshot 只保存标量 counter，并在调用时遍历当前 page；没有 event buffer、histogram
  或默认 diagnostics allocation。
- 在现有 real headless renderer test 中增加固定 TEXT-001 runner。workload 使用 Lilex、
  OpenMoji 和当前主机 fallback font，覆盖 Latin、1024 个 CJK 字符、combining mark、
  ligature、RTL、emoji、SVG、8 帧 image、14/16/20 px 与 1.0/1.25/2.0 scale。

基线验证：

| 命令或检查                                                                                         | 结果               | 说明                                                                               |
| -------------------------------------------------------------------------------------------------- | ------------------ | ---------------------------------------------------------------------------------- |
| `cargo check --locked -p gpui`                                                                     | `PASS`             | dev profile 7.19 s                                                                 |
| `cargo test --locked -p gpui`                                                                      | `PASS`             | 232 unit + 1 integration，0 failed                                                 |
| `cargo test --locked -p gpui_wgpu --test headless_renderer`                                        | `COVERAGE MISSING` | 测试目标编译通过，但未启用 `test-support`，0 tests                                 |
| `cargo test --locked -p gpui_wgpu --features test-support --test headless_renderer -- --nocapture` | `PASS`             | RADV hardware 与 llvmpipe fallback，2 tests                                        |
| `./script/clippy -p gpui`                                                                          | `PASS`             | all-target/all-feature release clippy + philosophy                                 |
| `./script/clippy -p gpui_wgpu`                                                                     | `PASS`             | all-target/all-feature release clippy + philosophy                                 |
| `cargo test --locked -p gpui platform::atlas_tests`                                                | `PASS`             | 2 snapshot/cache/removal tests                                                     |
| `cargo test --locked -p gpui_wgpu wgpu_atlas::tests`                                               | `PASS`             | 5 tests，含 residency/pending/upload snapshot                                      |
| `cargo test --locked -p gpui_wgpu --features test-support --test headless_renderer`                | `PASS`             | 2 runtime tests；TEXT-001 runner 明确 ignored                                      |
| `./script/clippy -p gpui -p gpui_wgpu`                                                             | `PASS`             | all-target/all-feature release clippy + philosophy                                 |
| `cargo check --locked -p gpui_macos --target x86_64-apple-darwin`                                  | `PASS`             | Metal atlas cross-target compile                                                   |
| `cargo check --locked -p gpui_windows --target x86_64-pc-windows-gnu --no-default-features`        | `FAIL (baseline)`  | `async-tar 0.6.1` 依赖未启用 `async-std/unstable`；错误发生在 DirectX atlas 编译前 |
| `cd docs && npx prettier --check src/development/text-rendering-refactor-progress.md`              | `PASS`             | progress ledger formatting                                                         |

TEXT-001 命令：

```sh
GPUI_TEXT_ATLAS_OUTPUT_DIR=/home/begonia/Documents/Github/Xero-Team/ZZZ/.tmp/text-rendering-refactor/phase-0 \
  cargo test --locked -p gpui_wgpu --features test-support \
  --test headless_renderer text_atlas_baseline_runner -- --ignored --nocapture
```

TEXT-001 结果：`PASS`

| 指标                         | RADV hardware | llvmpipe fallback |
| ---------------------------- | ------------- | ----------------- |
| unique atlas entries         | 9,502         | 9,502             |
| resident pages               | 9             | 9                 |
| resident bytes               | 34,603,008    | 34,603,008        |
| pending/uploaded bytes       | 21,503,356    | 21,503,356        |
| upload calls                 | 9,502         | 9,502             |
| cold group p50               | 8.713 ms      | 8.783 ms          |
| cold group p95               | 12.962 ms     | 12.898 ms         |
| warm lookup，全部 9,502 keys | 2.684 ms      | 2.733 ms          |
| upload flush + empty frame   | 104.002 ms    | 88.187 ms         |

原始 artifact：

- `.tmp/text-rendering-refactor/phase-0/text-001-hardware.json`，SHA-256
  `e4b72115f648d951e1cdbc67c1d2a4d1e260af777d0051fe08c27d89485f87b9`
- `.tmp/text-rendering-refactor/phase-0/text-001-fallback.json`，SHA-256
  `d37046cd5fd27fee800083a34541e83f5d79d23e6f61d7e7410ab2d5cd417eb9`

阶段 0 结论：baseline、指标定义、固定 seed、hardware/fallback workload 和原始数据均已
固定。当前实现的一次 cold workload 会为 9,502 entries 发出 9,502 次 upload；该数据是
TEXT-010 的对照基线，不代表阶段 8 必须保留 batching。

提交：`8f773b777d5af8829fc20efb5b3f53cf2e0f90ac`（signed）。

下一步：进入阶段 1，以 integer subpixel tick 和 `div_euclid`/`rem_euclid` 修复正负
glyph origin 分解，并完成 TEXT-002 unit 与 headless pixel coverage。

### 阶段 1：修复 subpixel quantization

状态：`COMPLETE`

已完成：

- 增加纯 `QuantizedGlyphCoordinate { integer, variant }` 和
  `quantize_glyph_coordinate`。先把 device coordinate 量化为 integer tick，再使用
  `div_euclid`/`rem_euclid` 分解，负坐标不会再把负 `fract()` 饱和为 variant 0。
- `paint_glyph` 的 x/y 都使用同一 helper；`SUBPIXEL_VARIANTS_Y = 1` 仍走同一路径。
- `paint_emoji` 使用同一 helper 的单 variant 模式，保留原 integer pixel alignment。
- unit tests 覆盖 -2.0 到 2.0 的全部 quarter-pixel tick、正负 tie、tie 邻域、整数、
  1.0/1.25/2.0 scale 和 variant 范围。
- 真实 headless pixel test 通过 `HeadlessAppContext → Window::paint_glyph →
CosmicTextSystem → WgpuHeadlessRenderer` 运行。device origin `-0.25` 与 `+0.75`
  使用同一 variant 3，输出在平移一个 device pixel 后逐像素完全相同。

TEXT-002：`PASS`

| 命令或检查                                                                                                                                                                                   | 结果   | 说明                                                    |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ | ------------------------------------------------------- |
| `cargo test --locked -p gpui glyph_coordinate_quantization`                                                                                                                                  | `PASS` | 2 unit tests                                            |
| `GPUI_HEADLESS_OUTPUT_DIR=... cargo test --locked -p gpui_wgpu --features test-support --test headless_renderer negative_subpixel_glyph_origin_matches_positive_phase_after_one_pixel_shift` | `PASS` | RADV hardware 与 llvmpipe fallback 逐像素相位检查       |
| `cargo test --locked -p gpui`                                                                                                                                                                | `PASS` | 234 unit + 1 integration，0 failed                      |
| `cargo test --locked -p gpui_wgpu --features test-support --test headless_renderer`                                                                                                          | `PASS` | 3 passed，TEXT-001 runner 1 ignored                     |
| `cargo check --locked -p gpui --target x86_64-apple-darwin`                                                                                                                                  | `PASS` | generic Window quantization cross-target compile        |
| `./script/clippy -p gpui -p gpui_wgpu`                                                                                                                                                       | `PASS` | all-target/all-feature release clippy + philosophy gate |
| `cargo fmt --all -- --check`                                                                                                                                                                 | `PASS` | workspace Rust formatting                               |

原始 pixel artifact：

- `.tmp/text-rendering-refactor/phase-1/text-002-hardware-negative.png`，SHA-256
  `0dbd262a2cec5677669ce119b2612644dd6ebef8405525a3c4ca49cdb62f719f`
- `.tmp/text-rendering-refactor/phase-1/text-002-hardware-positive.png`，SHA-256
  `1f6c261137865c9e131bb8e0a0be4f56c62366fbfb2a9beea34a17561775ef81`
- `.tmp/text-rendering-refactor/phase-1/text-002-fallback-negative.png`，SHA-256
  `a8fef174d500a8ae14f38f2624c03dd06d7030f30249164d5a5d95d79c80a225`
- `.tmp/text-rendering-refactor/phase-1/text-002-fallback-positive.png`，SHA-256
  `6b1ca1fb84252669271065d8bd229b39073adc0b4131fcf05378b970edeb4338`

提交：`c5dab076987e28d4af6d8824c27d993ceb97a0fd`（signed）。

下一步：进入阶段 2，先把 atlas domain 从 `platform.rs` 移到 `atlas.rs` 且保持 root
re-export，再集中 page allocator 和 lock 外 builder。

### 阶段 2：抽离 atlas domain 和公共 allocator

状态：`IN PROGRESS`

2A 状态：`COMPLETE`

- 新建单一逻辑组件 `crates/gpui/src/atlas.rs`，迁入 key、tile、texture kind、snapshot、
  state、backend contract、headless atlas、texture list 和 atlas unit tests。
- `gpui.rs` 从 crate root re-export atlas domain；现有 `gpui::AtlasTile`、
  `gpui::PlatformAtlas` 等公共路径保持不变，WGPU、Metal、DirectX consumer 无需修改。
- `platform.rs` 不再定义 atlas 类型或持有 atlas-only imports。

验证：

| 命令或检查                                                                          | 结果   | 说明                                  |
| ----------------------------------------------------------------------------------- | ------ | ------------------------------------- |
| `cargo test --locked -p gpui atlas::tests`                                          | `PASS` | 2 moved unit tests                    |
| `cargo check --locked -p gpui -p gpui_wgpu -p gpui_macos -p gpui_windows`           | `PASS` | root re-export consumers compile      |
| `cargo test --locked -p gpui`                                                       | `PASS` | 234 unit + 1 integration，0 failed    |
| `cargo test --locked -p gpui_wgpu --features test-support --test headless_renderer` | `PASS` | 3 passed，1 explicit baseline ignored |
| `./script/clippy -p gpui`                                                           | `PASS` | all-target/all-feature + philosophy   |
| `cargo fmt --all -- --check`                                                        | `PASS` | workspace Rust formatting             |

2A 提交：`ed5f4d49cb5569e7885c7ad315006a72caa8d3dc`（signed）。

2B 状态：`IN PROGRESS`

下一步把 `BucketedAtlasAllocator`、page size、page/entry metadata 和 monotonic identity
集中到公共 core；三个 backend 仅保留 GPU texture storage/upload/resource lookup，并把 miss
builder 移出 atlas lock 后 double-check insert。

### 阶段 3–9

状态：`NOT STARTED`

后续阶段严格按执行合同顺序推进；阶段 8 的实现可以由 TEXT-010 决定保留或拒绝，
其它阶段均为必做。
