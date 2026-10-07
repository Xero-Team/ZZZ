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
| 当前阶段      | 阶段 9 已完成                              |
| Goal 状态     | `COMPLETE`                                 |
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

状态：`COMPLETE`

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

2B 状态：`COMPLETE`

- 公共 `Atlas<Backend>` 现在唯一拥有 `BucketedAtlasAllocator`、page size、page/entry
  metadata、resident-page byte accounting、key lookup 和 allocation rollback。
- `AtlasTextureId` 与 `TileId` 在 atlas 实例生命周期内分别单调分配，不从 texture slot
  或 etagere `AllocId` 派生；remove、clear 和 device reset 后均不复用，溢出返回显式
  error 并回滚 allocation。
- WGPU、Metal、DirectX backend 使用 monotonic ID keyed storage，只实现 texture
  create/upload/destroy/clear 和 renderer resource lookup；backend crate 已删除 etagere
  dependency、free list 和 live-key counter。
- atlas miss 先在锁内查询，释放锁执行 raster/image builder，再重新加锁 double-check；
  concurrent miss 可以重复构建 bytes，但只产生一个 resident entry 和一次 upload。
- upload 或 ID allocation 失败会回滚 suballocation；oversized entry 不创建空 texture。
- clear/device reset 清空 resource lookup，但保留 ID counters，因此旧 texture identity
  不会解析到新 resource。

验证：

| 命令或检查                                                                                       | 结果              | 说明                                                                         |
| ------------------------------------------------------------------------------------------------ | ----------------- | ---------------------------------------------------------------------------- |
| `cargo test --locked -p gpui atlas::tests`                                                       | `PASS`            | 8 allocator/rollback/concurrency/identity/bounds tests                       |
| `cargo test --locked -p gpui`                                                                    | `PASS`            | 239 unit + 1 integration，0 failed                                           |
| `cargo test --locked -p gpui_wgpu --features test-support`                                       | `PASS`            | 16 unit passed、1 ignored；3 headless passed、1 runner ignored               |
| `cargo check --locked -p gpui -p gpui_wgpu -p gpui_macos -p gpui_windows`                        | `PASS`            | 当前 host package graph                                                      |
| `cargo check --locked -p gpui_macos --tests --target x86_64-apple-darwin`                        | `PASS`            | Metal atlas 与 tests cross-compile；仅既有 vendor warnings                   |
| isolated `cargo check --target x86_64-pc-windows-gnu --tests --offline` for `directx_atlas.rs`   | `PASS`            | `.tmp/text-rendering-refactor/windows-crosscheck/`                           |
| full `cargo check --locked -p gpui_windows --target x86_64-pc-windows-gnu --no-default-features` | `FAIL (baseline)` | 首先缺少 `async-std/unstable`；启用后暴露既有 `windows-core 0.62/0.100` 冲突 |
| `./script/clippy -p gpui -p gpui_wgpu -p gpui_macos -p gpui_windows`                             | `PASS`            | all-target/all-feature release clippy + philosophy                           |
| `cargo fmt --all -- --check`                                                                     | `PASS`            | workspace Rust formatting                                                    |
| backend allocator-policy search                                                                  | `PASS`            | backend 中无 etagere、free-list 或 live-key policy                           |

阶段 2 TEXT-001 复跑：`PASS`。hardware 与 fallback 仍为 9 pages、34,603,008
resident bytes、9,502 entries、21,503,356 uploaded bytes 和 9,502 upload calls，证明
allocator 集中化没有改变 baseline residency/upload semantics。

原始 artifact：

- `.tmp/text-rendering-refactor/phase-2/text-001-hardware.json`，SHA-256
  `44914c05eadb40a9b3b44f238118cc5a43979c02b4d05e4f7183ca60451ec35d`
- `.tmp/text-rendering-refactor/phase-2/text-001-fallback.json`，SHA-256
  `77c78f6856c02acbecd48c3fb2342747857f3e40979a02899bc7064cb1c600e3`

2B 提交：`c84404c1e8b59e596fd548a1d3d8bd00a2f6b6e4`（signed）。

下一步：阶段 3 增加 `AtlasEpoch`、`AtlasFrameId`、pending removal、retirement 和
completed-frame usage；remove 不再 deallocate，epoch mismatch 必须禁用 cached paint
replay。

### 阶段 3：frame-aware identity 和 retirement

状态：`COMPLETE`

- 增加 `AtlasEpoch`、`AtlasFrameId`、`AtlasFrame` 和 `AtlasUsage`。`Scene` 在 visible
  sprite insert、cached replay 和 clear 中维护去重的 tile/page usage；`Frame` 与
  `BuiltFrame` 保存 completed-frame epoch。
- `Window::draw` 在 build 前调用 atlas `begin_frame`，epoch mismatch 时 `force_refresh()`；
  completed Scene finish 后调用 `finish_frame` 并记录 epoch。cached paint replay 在 epoch
  变化的 frame 为零，epoch 不变时继续命中。
- `remove` 只进入 pending removal；下一 frame boundary 才从 key lookup 移除、标记
  tombstone、推进 epoch。frame build 中到达的 remove 留到下一 boundary，epoch 不会在
  build 中变化。
- retired suballocation 不调用 allocator deallocate。只在 page 没有 active entry 且新
  completed usage 不引用该 page 时整页销毁；partial page 的 retired pixels 保持原内容，
  不会被 replacement upload 覆盖。
- atlas clear/device reset 清空 resource lookup、推进 epoch 并保留 monotonic ID counters；
  WGPU upload flush 与 atlas frame boundary 使用不同方法名和职责。
- `Window::drop_image` 无论是否在 draw 中调用都会请求下一 full refresh；`App::drop_image`
  不再静默丢弃 Result，并为每个 window 独立排队 removal/refresh。

必须序列覆盖：

- A completed frame → remove A → old content remains valid until next completed usage。
- remove A → insert same-size B：suballocation identity/bounds 不复用，旧 Scene 仍为 A。
- fully retired page → new page uses new texture ID；旧 Scene lookup becomes empty, never B。
- remove during paint → next retirement frame is scheduled。
- image removal across two windows → each window advances its own epoch and full repaints。
- repeated present after completed refresh → no extra build and epoch remains unchanged。
- clear/device recovery → epoch advances and old texture ID cannot resolve to new resource。

TEXT-003：`PASS`

| 命令或检查                                                                                     | 结果   | 说明                                                           |
| ---------------------------------------------------------------------------------------------- | ------ | -------------------------------------------------------------- |
| `cargo test --locked -p gpui --features frame-diagnostics`                                     | `PASS` | 250 unit + 1 integration，含 replay/multi-window/during-paint  |
| `cargo test --locked -p gpui_wgpu --features test-support`                                     | `PASS` | 17 unit passed、1 ignored；4 headless passed、1 runner ignored |
| `GPUI_HEADLESS_OUTPUT_DIR=... cargo test ... retired_tiles_never_sample_replacement_content`   | `PASS` | RADV + llvmpipe；suballocation 与 full-page ABA pixel matrix   |
| `cargo check --locked -p gpui_macos --tests --target x86_64-apple-darwin`                      | `PASS` | Metal lifecycle/tests cross-compile；仅既有 vendor warnings    |
| isolated `cargo check --target x86_64-pc-windows-gnu --tests --offline` for `directx_atlas.rs` | `PASS` | DirectX lifecycle/tests cross-compile                          |
| `./script/clippy -p gpui -p gpui_wgpu -p gpui_macos -p gpui_windows`                           | `PASS` | all-target/all-feature release clippy + philosophy             |
| `cargo fmt --all -- --check`                                                                   | `PASS` | workspace Rust formatting                                      |

TEXT-003 artifacts 位于 `.tmp/text-rendering-refactor/phase-3/`，hardware/fallback
结果逐像素一致：

- retired suballocation old content：SHA-256
  `79220e9e48837f320e832da4691dea438fb254b1af412698c2bef0cbb159d57e`
- fully retired page old lookup after finish（transparent）：SHA-256
  `61b8acd53d0cab4840ac206d821a1b150c826f9feaa3dc6c075ad4308d1f4d56`
- replacement content：SHA-256
  `fd818a5e929cddefd9264d2615b0b684e2075253c23e3910f72897071e757e01`

提交：completed-frame usage `55736706bf44bebb225d438111dba3de4c012118`；
epoch/retirement/cache invalidation `0e2ddebaceee7fa0d5486211b74d3cfcd85b9160`（均 signed）。

下一步：阶段 4 引入 `AtlasContentKind`、独立 retained-cache budget、working-set floor、
LRU page retirement 和 whole-page compaction，并完成 TEXT-004/005/006。

### 阶段 4：content-class budget、LRU 和 compaction

状态：`COMPLETE`

- 增加 `AtlasContentKind::{GlyphAlpha, GlyphSubpixel, GlyphColor, SvgMask, Image}`；
  `AtlasTextureKind` 只决定 GPU pixel format。即使 texture format 相同，glyph/image、
  glyph/SVG 也使用不同 page pool、budget、recency 和 diagnostics。
- 增加 `AtlasPolicy`，production defaults 按 TEXT-001 baseline 保留现有常用 workload：
  GlyphAlpha 16 MiB、GlyphSubpixel 32 MiB、GlyphColor 32 MiB、SvgMask 8 MiB、Image
  64 MiB；test 可注入 page size 和每类 retained budget，不新增用户 setting。
- page 记录 content kind、resident bytes、active/tombstone entries 和 last-used frame；
  snapshot 提供每类 page/bytes/entries/budget/working-set/eviction/compaction/pressure。
- budget maintenance 只在 atlas frame boundary 执行。当前 completed usage 构成
  working-set floor；可见 working set 超 budget 时完整保留并记录 pressure，不返回空 tile。
- 超预算时按 content class 和 page recency 退休 cold entries/pages；无 cross-class eviction。
  cold entry 已不在 completed Scene，finish-frame retirement 不推进 epoch。
- 仍超预算时每个 frame 最多调度一个 sparse page whole-page compaction：hot lookup 退休，
  下一 full repaint 按需重新 raster/upload 到新 monotonic identity。旧 page 在新 completed
  usage 不引用后销毁；不在旧 page 上就地覆盖。
- compaction raster/allocation/upload 失败会恢复旧 lookup、active count 与正确 resident
  content，避免把失败变成缺字；单元测试覆盖 raster failure rollback。

TEXT-004：`PASS`

- hardware 与 fallback 各运行 10,000 个 CJK scalar × 14/16/20 px ×
  1.0/1.25/2.0 scale，共 711 frames 和 89,991 unique atlas keys。
- injected GlyphSubpixel budget 1,048,576 bytes，page 262,144 bytes；maximum resident 与
  maximum working set 均为 1,572,864 bytes，满足
  `max(budget, working set) + one page`。
- 每帧真实 WGPU render 后逐 8×8 cell 检查非空 coverage；0 missing/wrong glyph。
- hardware p95 30.830 ms，fallback p95 30.252 ms；两者最终 resident 262,144 bytes、
  eviction 89,975、whole-page compaction 77、pressure frames 383。

TEXT-005：`PASS`

- image retained budget 为 0，单 frame visible working set 为 4 个独立 8×8 pages，
  resident/working-set 均为 1,024 bytes，pressure=1、eviction=0。
- hardware/fallback pixel output 完全相同，四个 visible tiles 全部保留。

TEXT-006：`PASS`

- hardware/fallback 各运行 128 frames 的 glyph、SVG、animated image frame 和 128×128
  large-image churn，并逐帧验证可见 glyph/SVG/image pixels。
- 最终 GlyphSubpixel eviction=0；Image eviction=255；SvgMask eviction=124；最终各类
  resident bytes 分别 16,384 / 16,384 / 4,096，证明大 image 与 SVG pressure 不驱逐
  glyph working set。

验证：

| 命令或检查                                                                                     | 结果   | 说明                                                            |
| ---------------------------------------------------------------------------------------------- | ------ | --------------------------------------------------------------- |
| `cargo test --locked -p gpui atlas::tests`                                                     | `PASS` | 15 budget/floor/isolation/compaction/failure tests              |
| `cargo test --locked -p gpui --features frame-diagnostics`                                     | `PASS` | 255 unit + 1 integration，0 failed                              |
| `cargo test --locked -p gpui_wgpu --features test-support`                                     | `PASS` | 17 unit passed、1 ignored；5 headless passed、3 runners ignored |
| `text_atlas_budget_runner -- --ignored --nocapture`                                            | `PASS` | TEXT-004 RADV + llvmpipe，32.37 s                               |
| `working_set_over_budget_renders_without_missing_tiles`                                        | `PASS` | TEXT-005 RADV + llvmpipe pixel                                  |
| `text_atlas_content_isolation_runner -- --ignored --nocapture`                                 | `PASS` | TEXT-006 RADV + llvmpipe，1.00 s                                |
| `text_atlas_baseline_runner -- --ignored --nocapture`                                          | `PASS` | phase-0 workload remains 9 pages/34,603,008 bytes/9,502 uploads |
| `cargo check --locked -p gpui_macos --tests --target x86_64-apple-darwin`                      | `PASS` | Metal policy contract cross-compile；仅既有 vendor warnings     |
| isolated `cargo check --target x86_64-pc-windows-gnu --tests --offline` for `directx_atlas.rs` | `PASS` | DirectX policy contract cross-compile                           |
| `./script/clippy -p gpui -p gpui_wgpu -p gpui_macos -p gpui_windows`                           | `PASS` | all-target/all-feature release clippy + philosophy              |
| `cargo fmt --all -- --check`                                                                   | `PASS` | workspace Rust formatting                                       |

原始 artifact：

- TEXT-004 hardware/fallback JSON：
  `09e9d5de93d1fec265d22599b2a5f1507a8e4133347fc54d185d86d0d37d7d1b` /
  `7f0b05a080fe31bba9207006bd2b4b82dbf02dc4e354f1bcaaf9b3e7002aea44`
- TEXT-005 hardware/fallback PNG：相同 SHA-256
  `0c5c8c2d8550670cbffb81d5bbd17b2868a70543bf88115ee34b815c61c398e9`；对应 JSON
  `8f6fd437626128355029df146ec19d26f75b9065be497b54ff` /
  `2d464d5b83365583e3c6790cf25878453d86239774230edbf0e49c3f471816eb`
- TEXT-006 hardware/fallback JSON：
  `5f2c86330781b90ca2b2a97443ca378e78f59b5a64c9ab55dd76d58f48c1649a` /
  `b773a2f541280245f4ce60173daab718ca12c65891305d676c81ee80543d9f99`
- TEXT-006 hardware/fallback PNG：
  `cf7d1442d0c185e231b24d0a9c8a49ee5a063284265ebe2eaa038999f45d0b59` /
  `d5a6018ded3960d0659c3121215ee18028f80731dc41e4dbd6cebc9f0fde70cf`

提交：content-class separation `162932b0c68aae3bf438bef765fdabad22383dfe`；
budget/LRU/compaction `76c1dfea7dca29c4a4d4fc3b392a029fe22a5dbc`（均 signed）。

下一步：阶段 5 引入明确的 `GlyphRasterFormat`/`GlyphRasterInfo`/`RasterizedGlyph`，让实际
raster result 决定 atlas format，并完成 WGPU/Windows/macOS color-font 路径与 TEXT-007。

### 阶段 5：format-aware glyph raster contract

状态：`COMPLETE`

已完成：

- 增加 `GlyphRasterFormat`、`GlyphRasterInfo` 和 `RasterizedGlyph`；platform bounds 与
  pixel 查询共享同一 format-aware contract，并验证两次查询的 metadata 不漂移。
- `AtlasKey::Glyph` 包含 rasterizer 返回的实际 format；`Window::paint_glyph` 与
  `paint_emoji` 共享内部 paint 路径。`Alpha8`、`SubpixelBgra8`、`ColorBgra8` 分别进入
  monochrome、subpixel、untinted polychrome sprite。
- WGPU 以 Swash `Image::content` 为权威。OpenMoji 即使带 emoji source hint，实际
  monochrome mask 仍保持 `Alpha8` 和可 tint。
- Swash 0.2.10 不支持 COLRv1 或 OpenType-SVG。系统 FreeType 头文件也明确声明
  `FT_LOAD_COLOR` 不渲染 COLRv1，因此没有加入无效的 FreeType 回退。Linux/FreeBSD
  改用纯 Rust `skrifa` + `vello_cpu`/`glifo` 解释 COLRv1 paint graph，并用
  `skrifa` + `usvg`/`resvg` 渲染 `SVG ` glyph document；没有 Skia/rust-skia。
- WGPU 内部 pending image 在缓存前已统一为最终 contract：color pixels 是
  straight-alpha BGRA8，premultiplied RGBA 只转换一次。
- 公共 macOS/GPUI RGBA conversion 也改为透明像素 RGB 清零和整数四舍五入，与
  WGPU/Windows 的 straight-alpha contract 一致，并有独立 unit coverage。
- Windows 已改为 per-glyph color detection；macOS 已适配显式 raster contract。
- macOS 删除 Apple Color Emoji PostScript 白名单，改用 CoreText
  `kCTFontColorGlyphsTrait`。该 capability 统一决定保守 `ColorBgra8` raster、shaping
  hint 和 synthetic bold/italic 禁用；缺少 `m` 的 color font 也可加载。
- Windows color raster 优先使用 `IDWriteBitmapRenderTarget3`。COLRv1 走
  `DrawPaintGlyphRun`，COLRv0、SVG、PNG/JPEG/TIFF 和 premultiplied BGRA 走
  `DrawGlyphRunWithColorSupport`；native target 先清零、裁切真实 coverage，再只做一次
  premultiplied BGRA → straight-alpha BGRA 转换。旧系统保留现有 COLRv0 layer compositor
  与视觉正确的 monochrome-in-color fallback。
- 增加约 12 KiB 的 tracked licensed fixtures：COLRv1 U+1F600、CBDT/CBLC U+1F600 与
  OpenType-SVG U+1F680。`assets/fonts/text-rendering-fixtures/MANIFEST.md` 固定 source
  commit/package、source/output SHA-256、attribution 和 license；
  `script/build-text-rendering-fixtures` 可离线确定性重建子集。

TEXT-007：WGPU `PASS`；macOS/Windows native `NOT RUN`，精确 runbook 已固定。

| 检查                                                                                              | 结果   | 说明                                                                                       |
| ------------------------------------------------------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------ |
| `cargo test --locked -p gpui_wgpu --features test-support cosmic_text_system::tests`              | `PASS` | 14 tests；含 format authority、alpha/BGRA 与 SVG viewport                                  |
| `GPUI_TEXT_ATLAS_OUTPUT_DIR=... cargo test ... text_glyph_format_runner -- --ignored --nocapture` | `PASS` | tracked fixtures；RADV + llvmpipe；COLRv1、SVG、bitmap color、monochrome                   |
| `./script/clippy -p gpui_wgpu`                                                                    | `PASS` | all-target/all-feature release clippy + philosophy                                         |
| `cargo check --locked -p gpui_macos --tests --target x86_64-apple-darwin`                         | `PASS` | CoreText capability test cross-compile；native NOT RUN                                     |
| `./script/clippy -p gpui_windows`                                                                 | `PASS` | host graph + philosophy；Windows cfg 不在本机执行                                          |
| isolated Windows `IDWriteBitmapRenderTarget3` API crosscheck                                      | `PASS` | `x86_64-pc-windows-gnu` 类型检查                                                           |
| full `gpui_windows` cross-target check                                                            | `FAIL` | baseline：`async-tar` 缺 `async-std/unstable`；启用后为既有 `windows-core 0.62/0.100` 冲突 |
| `cargo test --locked -p gpui --features frame-diagnostics`                                        | `PASS` | 256 unit + 1 integration，0 failed                                                         |
| `cargo test --locked -p gpui_wgpu --features test-support`                                        | `PASS` | 21 unit passed、1 ignored；5 headless passed、4 runners ignored                            |
| `./script/clippy -p gpui -p gpui_wgpu -p gpui_macos -p gpui_windows`                              | `PASS` | all-target/all-feature release clippy + philosophy                                         |
| `script/check-licenses`                                                                           | `PASS` | tracked fixture licenses accepted                                                          |
| `cargo fmt --all -- --check`                                                                      | `PASS` | workspace Rust formatting                                                                  |

TEXT-007 artifact：

- `.tmp/text-rendering-refactor/phase-5/tracked/text-007-hardware.json`，SHA-256
  `3f858f5ebf64791497bc78bad0017122c0f47aa2e292b3b0885fb7ab5c17180f`
- `.tmp/text-rendering-refactor/phase-5/tracked/text-007-fallback.json`，SHA-256
  `a05df121e8ce87f61fc94f1028d2fcf50e86d810fda7bc61fb443ab8c8a4f680`

显式 raster contract 与 WGPU color-font 提交：
`58700e6e399f34355c7dd77bf288801ba8872ebe`（signed）。

macOS CoreText capability 提交：
`55dd1cd567368db1ce6416d586381cc8393692f5`（signed）。

Windows native color raster 提交：
`28ba8b9cf478fb09ad7139bb2f88821a5ca2e688`（signed）。

licensed fixtures 与 native runners 提交：
`9e0a22e8e063d1d7daec3c5980ccf83095745474`（signed）。

Windows native TEXT-007 runbook（当前主机 `NOT RUN`）：

```powershell
cargo test --locked -p gpui_windows color_font_fixture_runner -- --ignored --nocapture
```

macOS native TEXT-007 runbook（当前主机 `NOT RUN`）：

```sh
cargo test --locked -p gpui_macos color_font_fixture_runner -- --ignored --nocapture
```

阶段 5 结论：实际 raster result 已成为 atlas format 的唯一权威；普通 glyph 仍可 tint，
color glyph 不受 text color tint；WGPU 的 COLRv1、OpenType-SVG、bitmap color 与
monochrome fixtures 均通过硬件和 fallback pixel checks。三平台明确使用 straight-alpha
BGRA8，premultiplied conversion 只发生一次。macOS/Windows native 结果按合同保留为
`NOT RUN` 并提供可直接执行的 tracked-fixture runners。

### 阶段 6：padding、sampling 和边界正确性

状态：`COMPLETE`

已完成第一组实现：

- 所有 atlas entry 使用 1 device-pixel gutter。`AtlasTile::bounds` 继续表示 shader
  采样的 inner content；`AtlasEntry::allocation_bounds` 保存 allocator/backend upload 的
  outer bounds，`AtlasTile::padding == 1` 反映真实 gutter。
- padding bytes 在 builder 完成后、atlas mutex 重新加锁前生成；并发 miss 仍可重复构建，
  但 padding copy、font raster 和 image copy 都不在 atlas lock 内执行。
- alpha/subpixel/color glyph 与 SVG mask 使用 transparent gutter；ordinary image 使用
  edge-extruded gutter。公共 helper 覆盖 1-byte/4-byte row、exact input length 和
  checked size arithmetic。
- cropped image sub-tile 只修改 inner bounds，保留 texture/tile identity 与 padding。
- WGPU/Metal 保留 linear sampling；DirectX atlas sampler 从 wrap 改为 clamp。
- resident/working-set page bytes 仍由真实 texture page 计算；pending/uploaded bytes
  现在包含 gutter bytes。

验证：

| 检查                                                                                                       | 结果   | 说明                                                        |
| ---------------------------------------------------------------------------------------------------------- | ------ | ----------------------------------------------------------- |
| `cargo test --locked -p gpui atlas::tests`                                                                 | `PASS` | 19 tests；含 inner/outer bounds、transparent/extruded bytes |
| `cargo test --locked -p gpui --features frame-diagnostics`                                                 | `PASS` | 260 unit + 1 integration，含 cropped sub-tile identity      |
| `cargo test --locked -p gpui_wgpu --features test-support`                                                 | `PASS` | 21 unit passed、1 ignored；5 headless passed                |
| `GPUI_TEXT_ATLAS_OUTPUT_DIR=... cargo test ... text_atlas_sampling_gutter_runner -- --ignored --nocapture` | `PASS` | RADV + llvmpipe；integer/fractional/scale/rotation matrix   |
| `cargo check --locked -p gpui_macos --tests --target x86_64-apple-darwin`                                  | `PASS` | Metal upload path cross-compile                             |
| `./script/clippy -p gpui -p gpui_wgpu -p gpui_macos -p gpui_windows`                                       | `PASS` | all-target/all-feature + philosophy                         |

TEXT-008：`PASS`

- ordinary image 红/蓝高对比相邻 tile 覆盖 integer、fractional、non-uniform scale；
  integer baseline 的完整 sprite edge 与所有 interior samples 保持不透明原色，没有透明 seam
  或邻 tile 串色。
- alpha/subpixel/color glyph 与 SVG mask 的 transparent tile 紧邻 full-coverage tile；
  fractional、non-uniform scale 和 rotation 后整张输出仍为透明，没有 cross-tile bleed。
- RADV hardware 与 llvmpipe fallback 输出逐像素一致。

TEXT-008 artifact：

- `.tmp/text-rendering-refactor/phase-6/text-008-hardware.json`，SHA-256
  `ba918f4c252fe768db02414c6f11e6dfaa90038aa176db5e224b9b18ff5b0139`
- `.tmp/text-rendering-refactor/phase-6/text-008-fallback.json`，SHA-256
  `78f25ec8e98c1d94728533b88cc859a8efa13f622599c1ba3f9c4eee8fbd8101`
- hardware/fallback image matrix SHA-256 均为
  `20d1d9d59546bfcc1deadc4c8921536deba4225cc2d384a95343372a3391b8ff`
- hardware/fallback transparent matrix SHA-256 均为
  `565cd802339d69a4ac19979f0ece95fe5d09ae3d6ab91c4cee9b52a26271a644`

gutter 与 sampler 提交：
`c03f3748b37d8bac9ea895890115bf12b9ca224c`（signed）。

TEXT-008 pixel matrix 提交：
`92dcd557669fe56aeb6770e2482be10b251626f9`（signed）。

阶段 6 结论：inner content bounds 没有因 padding 增大，outer allocation/upload bytes
包含真实 gutter；普通 image 的 extruded edge 消除了 seam，glyph/SVG 的 transparent
gutter 阻断邻接采样。TEXT-008 在当前可执行 backend 全部通过。

### 阶段 7：有预算的 strike/raster-info cache

状态：`COMPLETE`

已完成：

- flat application-lifetime map 替换为 `GlyphStrikeKey → PackedGlyphKey → GlyphRasterInfo`。
  strike key 包含 font、size bits、scale bits、synthetic style、emoji source hint、subpixel
  mode 与离散 dilation；packed key 保存 glyph id 和经过范围验证的 x/y subpixel variant。
- 默认同时限制 256 strikes 与 8 MiB estimated bytes。cache 使用稳定 strike slots、
  key-to-index map、hot strike index 与 whole-strike LRU；warm hot path 是短 `RwLock` read。
- platform raster-info query 在 lock 外执行，插入时 double-check。test-support 可注入
  byte/count limits、读取 snapshot，并独立开关 benchmark hit accounting。
- 第一版只缓存 metadata。删除 WGPU `pending_glyph_images` 与 Windows
  `pending_color_glyphs` 隐性 bitmap tier；atlas miss 时重新 rasterize，并继续验证
  info/pixel metadata 完全一致。
- unit coverage 包含 subpixel range、同 strike warm hit、whole-strike LRU、byte/count hard
  caps、eviction 后 cold equivalence，以及 zero-limit 无 replacement growth。

TEXT-009：`PASS`

| 指标                                     | 结果             |
| ---------------------------------------- | ---------------- |
| churn configured strike limit            | 24               |
| churn configured byte limit              | 65,536           |
| churn resident strikes / estimated bytes | 24 / 17,136      |
| churn whole-strike evictions             | 412              |
| warm resident strikes / entries          | 8 / 1,152        |
| warm estimated bytes                     | 69,952 / 131,072 |
| warm platform misses during timing       | 0                |
| flat-map warm lookup p95                 | 18,254 ns        |
| bounded-cache warm lookup p95            | 17,913 ns        |
| bounded / flat p95 ratio                 | 98.13%           |

TEXT-009 release artifact：

- `.tmp/text-rendering-refactor/phase-7/release/text-009.json`，SHA-256
  `33964c8c6cd0d8f6fc8ec1a28752998feccd77e34ab64feaabe92ff50f034730`

验证：

| 检查                                                                      | 结果   | 说明                                         |
| ------------------------------------------------------------------------- | ------ | -------------------------------------------- |
| `cargo test --locked -p gpui raster_info_cache_tests`                     | `PASS` | 5 bounded-cache unit tests                   |
| `cargo test --locked -p gpui --features frame-diagnostics`                | `PASS` | 265 unit + 1 integration，0 failed           |
| `cargo test --locked -p gpui_wgpu --features test-support`                | `PASS` | 21 unit passed、1 ignored；5 headless passed |
| release `text_raster_info_cache_runner`                                   | `PASS` | p95 improves 1.87% vs flat-map baseline      |
| TEXT-007 / TEXT-008 regression rerun                                      | `PASS` | hardware + llvmpipe                          |
| `cargo check --locked -p gpui_macos --tests --target x86_64-apple-darwin` | `PASS` | cross-target metadata/reraster compile       |
| `./script/clippy -p gpui -p gpui_wgpu -p gpui_macos -p gpui_windows`      | `PASS` | all-target/all-feature + philosophy          |

阶段 7 结论：cache 的 count/estimated bytes 始终不超过注入限制，同 strike warm lookup
不重复查询 platform font state，eviction 后 metadata 与 cold raster 一致，且没有第二份
bitmap 或永久增长的 replacement map。

bounded raster-info cache 提交：
`f29ef80998c62aa2cbfb3aaf487d0f5c74c58003`（signed）。

### 阶段 8：upload batching 决策

状态：`COMPLETE — REJECTED`

TEXT-010 使用 phase-0 mixed Latin/CJK/emoji workload，在 RADV 与 llvmpipe 上分别运行
5 次 cold-cache per-entry baseline 与 single-staging-buffer prototype。prototype 使用一个
transient `COPY_SRC` buffer 和多条 buffer-to-texture copy，没有 full-page CPU mirror。

TEXT-010：`REJECTED`

| 指标                          | RADV hardware               | llvmpipe fallback           |
| ----------------------------- | --------------------------- | --------------------------- |
| allocations / pending regions | 9,520                       | 9,520                       |
| per-entry upload calls        | 9,520                       | 9,520                       |
| batched upload calls          | 1                           | 1                           |
| logical upload bytes          | 25,140,440                  | 25,140,440                  |
| batched submitted bytes       | 60,598,784                  | 60,598,784                  |
| byte amplification            | 2.4104×                     | 2.4104×                     |
| per-entry frame p95           | 2,634,702,984 ns            | 2,669,681,261 ns            |
| batched frame p95             | 2,645,214,282 ns            | 2,607,770,467 ns            |
| frame p95 ratio               | 100.40%                     | 97.68%                      |
| gate                          | `FAIL`：bytes amplification | `FAIL`：bytes amplification |

upload calls 显著下降且 frame p95 没有超过 5% 回退，但 WebGPU
`COPY_BYTES_PER_ROW_ALIGNMENT = 256` 使大量小 glyph region 的 submitted bytes 放大到
2.41×，超过 1.5× 实验门槛。WGPU 首关失败，因此没有把 Linux 结果外推到 Metal/DirectX。
prototype、test toggles 与 staging code 已全部删除；生产仍使用 per-entry
`queue.write_texture`，没有遗留双路径或 CPU atlas mirror。

TEXT-010 artifact：

- `.tmp/text-rendering-refactor/phase-8/text-010.json`，SHA-256
  `1cbe80ee59a786d076da896fe0c05ac12074c6908115f9e388b833ab1dc08ddf`

### 阶段 9

状态：`COMPLETE`

阶段 9 完成旧 atlas 迁移命名、双路径和 staging prototype 清理，更新最终 architecture、
implementation evidence、实验状态和跨平台欠账。production 继续使用 per-entry
`queue.write_texture`；没有 Skia/rust-skia production dependency、默认网络下载、未说明
feature flag、第二份 bitmap cache 或永久 TODO。

TEXT-011：`PASS`

- 使用既有 `frame_diagnostics_runner` 在计划基线
  `a3a0f9734069b543f3fe1e0bdd77a37fbd1b2b31` 和最终分支各运行 5 次。每次包含 100 个
  dirty render、100 个 cached replay、100 个 focus/input render，共 301 次 draw。
- baseline `draw_p95` 中位数为 100,118 ns；最终中位数为 93,276 ns；比例
  `93.17%`，通过 `<= 105%` 门槛。
- baseline input p95 中位数为 128,162 ns；最终中位数为 126,799 ns；比例
  `98.94%`。
- `gpui` tests 覆盖 input/focus、appearance、resize/scale change、cached replay 和 atlas
  clear/recovery epoch contract；WGPU hardware 与 llvmpipe headless tests 覆盖 pixel、budget、
  color font、gutter、bounded cache 和真实 WGPU context rebind。

最终 Linux/WGPU 验证：

| 命令或检查                                                                                          | 结果               | 说明                                                                                   |
| --------------------------------------------------------------------------------------------------- | ------------------ | -------------------------------------------------------------------------------------- |
| `cargo fmt --all -- --check`                                                                        | `PASS`             | workspace Rust formatting                                                              |
| `./script/clippy`                                                                                   | `PASS`             | philosophy、workspace release/all-target/all-feature、`future_not_send`                |
| `./script/check-philosophy`                                                                         | `PASS`             | 无 commercial/account/telemetry/default-network surface                                |
| `script/check-licenses`                                                                             | `PASS`             | tracked color fixtures 和依赖 license                                                  |
| `script/check-todos`                                                                                | `PASS`             | 无未说明 TODO                                                                          |
| `script/check-keymaps`                                                                              | `PASS`             | keymap gate                                                                            |
| `script/check-links local`                                                                          | `PASS`             | 0 errors                                                                               |
| `cd docs && npx prettier --check src/`                                                              | `PASS`             | 全 docs tree                                                                           |
| `cargo test --locked -p gpui`                                                                       | `PASS`             | 258 unit + 1 integration                                                               |
| `cargo test --locked -p gpui --features frame-diagnostics`                                          | `PASS`             | 265 unit + 1 integration                                                               |
| `cargo test --locked -p gpui_wgpu`                                                                  | `PASS`             | 22 passed、1 compositor-only ignored；无 feature 时 headless target 为 0 tests         |
| `cargo test --locked -p gpui_wgpu --features test-support`                                          | `PASS`             | 22 unit passed、5 headless passed、7 intentional ignored                               |
| `cargo test --locked -p gpui_wgpu --test headless_renderer`                                         | `PASS`             | 合同要求的无 feature 编译检查，0 tests                                                 |
| `cargo test --locked -p gpui_wgpu --features test-support --test headless_renderer`                 | `PASS`             | hardware + llvmpipe，5 passed、6 artifact runners ignored                              |
| `text_atlas_baseline_runner -- --ignored --nocapture`                                               | `PASS`             | 9,520 entries；RADV/llvmpipe 11 pages、42,991,616 bytes、9,520 uploads                 |
| `text_atlas_budget_runner -- --ignored --nocapture`                                                 | `PASS`             | 89,991 CJK keys；最大 resident/working set 2 MiB，在 budget + working-set allowance 内 |
| `text_atlas_content_isolation_runner -- --ignored --nocapture`                                      | `PASS`             | glyph eviction 0；image 255；SVG 127                                                   |
| `text_glyph_format_runner -- --ignored --nocapture`                                                 | `PASS`             | COLRv1、SVG、bitmap color、monochrome；RADV + llvmpipe                                 |
| `text_atlas_sampling_gutter_runner -- --ignored --nocapture`                                        | `PASS`             | hardware/fallback 无 bleed、无 transparent seam                                        |
| release `text_raster_info_cache_runner -- --ignored --nocapture`                                    | `PASS`             | final p95 ratio `104.39%`，低于 105%；24 strikes / 65,536-byte limits                  |
| `wgpu_atlas::tests::device_lost_rebinds_without_reusing_atlas_identity`                             | `PASS`             | 销毁旧 device、绑定新 context；epoch/identity/pending uploads 正确                     |
| `cargo check --locked -p gpui_macos --tests --target x86_64-apple-darwin`                           | `PASS`             | native runtime `NOT RUN`；仅既有 vendor warnings                                       |
| `cargo check --locked -p gpui_windows --tests --target x86_64-pc-windows-gnu --no-default-features` | `FAIL (baseline)`  | `async-tar` 依赖缺少 `async-std/unstable`；在 gpui_windows 本体前失败                  |
| `cargo test --workspace`                                                                            | `BASELINE FAILURE` | editor/keymap exact failures 与 16 个失败 target 均在计划基线复现                      |

`cargo test --workspace` 运行 33 分钟后，`editor` 测试二进制仍有 3 个用例持续满核运行且无
进展，因此中断并逐项复现。最终分支和 detached 计划基线 worktree 上一致的 editor
例外为：

| 类别         | 用例                                                                                 |
| ------------ | ------------------------------------------------------------------------------------ |
| 稳定失败     | `editor_tests::test_auto_formatter_skips_server_without_formatting`                  |
| 稳定失败     | `inlays::inlay_hints::tests::test_no_hint_duplication_when_refresh_races_with_fetch` |
| 稳定失败     | `editor_tests::test_document_format_during_save`                                     |
| 稳定失败     | `editor_tests::test_format_echoing_received_line_endings_keeps_cursor`               |
| 稳定失败     | `editor_tests::test_join_lines_rust_block_comments`                                  |
| 稳定失败     | `editor_tests::test_multibuffer_format_during_save`                                  |
| 75 s timeout | `editor_tests::test_range_format_on_save_success`                                    |
| 75 s timeout | `editor_tests::test_range_format_respects_language_tab_size_override`                |
| 75 s timeout | `editor_tests::test_race_in_multibuffer_save`                                        |

`keymap_editor::tests::test_modifier_search_keeps_matching_shortcuts_after_release` 也在两边以
相同 `zed::OpenKeymap` action 缺失失败。

随后在最终分支和计划基线分别运行相同的 `cargo test --workspace --no-fail-fast`，跳过上述
editor/keymap 例外和单独已通过的 hover flake。两边得到完全相同的 16 个稳定失败 target：

```text
lsp_command_selector --lib
markdown --lib
migrator --lib
outline_panel --lib
project --test integration
project_panel --lib
project_symbols --lib
remote_server --lib
repl --lib
settings --lib
settings_profile_selector --lib
sidebar --lib
task --lib
tasks_ui --lib
util --lib
worktree --test integration
```

当前分支该次运行还短暂报告 `markdown_preview --lib`，但单独复跑 25/25 通过；此前的
`hover_links::tests::test_hover_markdown_link_with_row_column` 也单独通过，二者记录为并发
flake，不列入稳定 baseline 集合。所有失败 target 均不在本分支 diff 中。

上述逐项和完整 target-set 复现满足执行合同“只剩有复现记录的既有 baseline failure”
条件。

TEXT-011 artifact：

- `.tmp/text-rendering-refactor/phase-9/text-011.json`，SHA-256
  `6ef1ccc4b990ffc7d3988435d1bcca8be16ba4f98094a716460d309c2e3f051b`
- 阶段 9 的 TEXT-001/004/006/007/008/009 hardware/fallback artifacts 位于
  `.tmp/text-rendering-refactor/phase-9/final/`。

| Artifact          | SHA-256                                                            |
| ----------------- | ------------------------------------------------------------------ |
| TEXT-001 hardware | `93546b864a4ded54476f98e04a38eeca581c4a53e91b6deeec3f68a371b181f9` |
| TEXT-001 fallback | `70f452972841a34f5ea668513c73e59088b532f1542cd56c40722ab2a30609e5` |
| TEXT-004 hardware | `a4304baa5567253e8fd7ee20b9e95223d382e0fff16097729e0db6c7d4bb6f48` |
| TEXT-004 fallback | `bb595012fc827d59864f09fd04a725547a3a3430e64c954422e583b596fe11db` |
| TEXT-006 hardware | `ff510b90268afcc1d95c10baa0209d086ea4018f90dc25bed6dcacae09d82831` |
| TEXT-006 fallback | `03bad686bdbea0e23d5d0aea0676e8280ebd790f044a352e9a6b8c4ef2c98c2f` |
| TEXT-007 hardware | `3f858f5ebf64791497bc78bad0017122c0f47aa2e292b3b0885fb7ab5c17180f` |
| TEXT-007 fallback | `a05df121e8ce87f61fc94f1028d2fcf50e86d810fda7bc61fb443ab8c8a4f680` |
| TEXT-008 hardware | `ba918f4c252fe768db02414c6f11e6dfaa90038aa176db5e224b9b18ff5b0139` |
| TEXT-008 fallback | `78f25ec8e98c1d94728533b88cc859a8efa13f622599c1ba3f9c4eee8fbd8101` |
| TEXT-009 release  | `4a865090dd8a18d93eb0027a98d95ce48c34643e19be6a2065e017ed5284067c` |

平台矩阵：

| 平台                | 状态      | 结论                                                                 |
| ------------------- | --------- | -------------------------------------------------------------------- |
| Linux/WGPU hardware | `PASS`    | RADV unit、headless pixel、CJK budget、color fixture、TEXT-011 smoke |
| Linux/WGPU fallback | `PASS`    | llvmpipe pixel、budget、color、gutter、device-reset atlas contract   |
| macOS/Metal         | `NOT RUN` | tests cross-compile 通过；native color、pixel、scale runtime 不可用  |
| Windows/DirectX     | `NOT RUN` | full cross-target 受既有依赖阻断；native color/device-lost 不可用    |

macOS native runbook（当前 Linux 主机 `NOT RUN`）：

```sh
cargo test --locked -p gpui_macos -- --nocapture
cargo test --locked -p gpui_macos color_font_fixture_runner -- --ignored --nocapture
RUST_LOG=gpui_macos=debug,gpui=debug cargo run --profile release-fast 2>&1 | tee /tmp/zzz-macos-text-rendering.log
```

在 tracked COLRv1、bitmap、OpenType-SVG 和 OpenMoji fixture 上验证 color/monochrome；在
Retina 与非 Retina display 间移动窗口并切换 display scale，完成编辑、滚动、resize 和
appearance change。采集原始/变化后截图、`/tmp/zzz-macos-text-rendering.log` 和 Metal GPU
capture。判定：没有 tint、缺字、bleed、seam 或 stale tile；scale change 后完整 repaint；
连续 5 次 frame diagnostics 的 draw p95 中位数不超过同机 baseline 的 105%。

Windows native runbook（当前 Linux 主机 `NOT RUN`；在可丢弃测试机运行 device reset）：

```powershell
cargo test --locked -p gpui_windows -- --nocapture
cargo test --locked -p gpui_windows color_font_fixture_runner -- --ignored --nocapture
$env:RUST_LOG = "gpui_windows=debug,gpui=debug"
cargo run --profile release-fast 2>&1 | Tee-Object -FilePath $env:TEMP\zzz-windows-text-rendering.log
dxcap -forcetdr
```

使用相同 tracked fixture 和普通 monochrome 文本，运行编辑、滚动、resize 与 display-scale
change；`dxcap -forcetdr` 后继续输入和滚动。采集 reset 前后截图、
`zzz-windows-text-rendering.log` 和 PIX capture。判定：DirectWrite color glyph 不 tint，
monochrome 仍可 tint；device lost 后 atlas epoch/full refresh 恢复，没有 stale tile、缺字、
bleed 或崩溃；连续 5 次 frame diagnostics 的 draw p95 中位数不超过同机 baseline 的
105%。

阶段 9 结论：全部必做阶段、实验、清理、当前主机 runtime 和文档已完成；native
macOS/Windows 欠账按合同精确标记 `NOT RUN`，没有未关闭 blocker、TODO 或 prototype。
