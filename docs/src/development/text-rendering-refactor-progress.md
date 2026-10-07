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
| 当前阶段      | 阶段 5 进行中                              |
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

状态：`IN PROGRESS`

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
- Windows 已改为 per-glyph color detection；macOS 已适配显式 raster contract。
- macOS 删除 Apple Color Emoji PostScript 白名单，改用 CoreText
  `kCTFontColorGlyphsTrait`。该 capability 统一决定保守 `ColorBgra8` raster、shaping
  hint 和 synthetic bold/italic 禁用；缺少 `m` 的 color font 也可加载。

TEXT-007 当前 WGPU 结果：`PASS`

| 检查                                                                                 | 结果   | 说明                                                      |
| ------------------------------------------------------------------------------------ | ------ | --------------------------------------------------------- |
| `cargo test --locked -p gpui_wgpu --features test-support cosmic_text_system::tests` | `PASS` | 14 tests；含 format authority、alpha/BGRA 与 SVG viewport |
| `GPUI_*_FONT=... cargo test ... text_glyph_format_runner -- --ignored --nocapture`   | `PASS` | RADV + llvmpipe；COLRv1、SVG、bitmap color、monochrome    |
| `./script/clippy -p gpui_wgpu`                                                       | `PASS` | all-target/all-feature release clippy + philosophy        |
| `cargo check --locked -p gpui_macos --tests --target x86_64-apple-darwin`            | `PASS` | CoreText capability test cross-compile；native NOT RUN    |

TEXT-007 artifact：

- `.tmp/text-rendering-refactor/phase-5/text-007-hardware.json`，SHA-256
  `56e7b6b50a8455afa2ea7f7a7772a08852548a647d09a7e639370a178f94ab66`
- `.tmp/text-rendering-refactor/phase-5/text-007-fallback.json`，SHA-256
  `48b3b2e897fe36771bb8acfe9e862bf82176d2e91c923bac8ddc6b90918ae04c`

显式 raster contract 与 WGPU color-font 提交：
`58700e6e399f34355c7dd77bf288801ba8872ebe`（signed）。

下一步：补齐 Windows SVG/bitmap/COLRv1 native raster 路径或精确 runbook，并固定
licensed fixture provenance/manifest 后关闭阶段 5。

### 阶段 6–9

状态：`NOT STARTED`

后续阶段严格按执行合同顺序推进；阶段 8 的实现可以由 TEXT-010 决定保留或拒绝，
其它阶段均为必做。
