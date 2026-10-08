---
title: Text Rendering Refactoring Plan
description: Staged execution contract for correcting and restructuring GPUI text rasterization and atlas lifetime.
---

# GPUI 文本渲染完整重构执行计划

本文把[架构研究报告](./report.md)转换为可持续执行、逐阶段验收和随时恢复的实施
合同。后续 agent 必须完成全部必做阶段，不能在只修一个 bug、只完成抽象或只提交
原型后停止。

- 计划基线：ZZZ `a3a0f9734069b543f3fe1e0bdd77a37fbd1b2b31`
- Skia 参考：`84068ebf574dd6c8503ac3e6a02a1ff2f0633fbd`
- rust-skia 参考：`594bb85bad45777ff5f7e284ba54c51346fb9e2e`
- 证据：[evidence.tsv](./evidence.tsv)
- 实验阈值：[experiments.tsv](./experiments.tsv)
- 可复制 Goal：[goal.md](./goal.md)
- 执行结果：[../text-rendering-refactor-progress.md](../text-rendering-refactor-progress.md)

> 执行状态（2026-10-07）：阶段 0–7 和阶段 9 已通过；阶段 8 的 batching prototype
> 已按 TEXT-010 数据拒绝并删除。Goal 已完成。本文件保留为不可缩减的历史执行合同，
> 最终结果和 native `NOT RUN` 欠账见执行账本。

如果执行开始时 HEAD 已前进，先审查相对本基线在目标 crate 上的差异。只要新提交
没有完成或否定本计划，就更新执行基线并继续；不得 reset、clean 或覆盖用户修改。

## 1. 目标和完成定义 {#goal-and-done}

重构完成后，GPUI 保持平台原生 shaping 和现有公开 façade，但具备：

- 正负坐标一致的 subpixel quantization；
- 不会发生 tile/texture identity ABA 的 atlas；
- 与 completed frame 和 cached paint replay 对齐的 epoch/retirement protocol；
- 后端无关的 page allocator、content-class budget、LRU 和 compaction；
- 由 raster result 决定的 monochrome/subpixel/color glyph format；
- linear sampling 安全的 gutter policy；
- 有 byte/count 限制的 strike/raster-info cache；
- 可测量的 memory、hit/miss、eviction、upload 和 frame-time diagnostics。

只有同时满足以下条件，Goal 才能标记完成：

1. 阶段 0 到阶段 7 和阶段 9 全部完成；阶段 8 得出“保留实现”或“拒绝实现”的
   数据化结论。
2. `TEXT-002` 和 `TEXT-003` 通过，没有负坐标跳动和旧 tile 采样新内容。
3. `TEXT-004`、`TEXT-005` 和 `TEXT-006` 通过，retained cache memory 收敛，同时
   单帧 working set 不缺字。
4. `TEXT-007` 和 `TEXT-008` 在当前可执行 backend 通过；macOS/Windows native
   runtime 若不可用，提供精确 runbook 并标记 `NOT RUN`。
5. atlas 的 allocation、budget、retirement 和 compaction 算法只有一份公共实现；
   WGPU、Metal、DirectX 不再各自维护独立 etagere allocator policy。
6. 不存在临时双实现、未说明 feature flag、永久 TODO、无删除条件 compatibility
   adapter 或无预算 cache。
7. 不增加 Skia/rust-skia production dependency，不增加默认网络下载。
8. 所有相关 tests、workspace tests、clippy、philosophy gate 和 docs formatting 通过，
   或只剩有复现记录的既有 baseline failure。
9. 最终架构文档、进度账本、实验结果和平台欠账已更新。

## 2. 范围 {#scope}

核心代码范围：

- `crates/gpui/src/atlas.rs`（新逻辑组件）
- `crates/gpui/src/platform.rs`
- `crates/gpui/src/frame.rs`
- `crates/gpui/src/scene.rs`
- `crates/gpui/src/view.rs`
- `crates/gpui/src/window.rs`
- `crates/gpui/src/text_system.rs`
- `crates/gpui/src/text_system/`
- `crates/gpui_wgpu/src/wgpu_atlas.rs`
- `crates/gpui_wgpu/src/cosmic_text_system.rs`
- `crates/gpui_wgpu/src/wgpu_renderer.rs` 及其子模块
- `crates/gpui_macos/src/metal_atlas.rs`
- `crates/gpui_macos/src/text_system.rs`
- `crates/gpui_macos/src/shaders.metal`
- `crates/gpui_windows/src/directx_atlas.rs`
- `crates/gpui_windows/src/direct_write.rs`
- `crates/gpui_windows/src/directx_renderer.rs`
- `crates/gpui_windows/src/shaders.hlsl`

必要时修改：

- headless/visual test infrastructure；
- 已有 font test assets 和 license manifests；
- `docs/src/development/text-layout.md`；
- GPU shader build definitions。

不属于本计划：

- 把 macOS/Windows shaping 迁移到 cosmic-text；
- 用 Skia 替换 GPUI renderer；
- 引入新的公开字体设置或 atlas budget setting；
- 重写 layout、bidi、font fallback 或 line wrapping；
- 实现 SDF text、vector glyph GPU tessellation 或 bindless texture system；
- 为 batching 重新排序可能重叠的透明 sprite；
- 顺手重构非文本 renderer、业务 UI 或主题系统。

## 3. 执行规则 {#execution-rules}

### 3.1 工作区和提交 {#workspace-and-commits}

1. 开始前读取根 `AGENTS.md`、`.rules`、`docs/AGENTS.md` 和本研究目录全部文件。
2. 确认 `git status`。保留用户修改，不得 reset、clean、checkout 丢弃或覆盖。
3. 在专用分支 `refactor/gpui-text-rendering` 工作。不要 push、开 PR 或合并 main。
4. 创建并持续维护
   `docs/src/development/text-rendering-refactor-progress.md`，记录 baseline、阶段、
   tests、实验、PASS/FAIL/BLOCKED/NOT RUN、提交和下一步。
5. 原始 benchmark、pixel image、trace 和临时 font fixture 放在
   `.tmp/text-rendering-refactor/`。只有可复现命令、摘要和决策进入 tracked docs。
6. 每个提交只做一个可独立回退的逻辑变化，使用 `git commit -s`。
7. 阶段结束不允许留下不编译状态；可选实验失败时删除原型，不保留死 feature。

### 3.2 代码质量 {#code-quality}

- 正确性和清晰度优先于微优化。
- 不新增 `unwrap()`；`expect()` 只用于真实 programmer invariant。
- 不以 `_ =`、`let _ =` 或 `.ok()` 静默吞掉 fallible operation。
- 使用完整变量名，不创建 `mod.rs`。
- `atlas.rs` 是一个新的逻辑组件；不要继续拆成多个只有少量代码的薄文件。
- builder、font rasterization 和 image copy 不得在 atlas mutex 内执行。
- 不引入 trait 只服务一个 backend；公共 trait 必须覆盖至少 WGPU、Metal、DirectX
  或形成真实 test seam。
- comments 只解释生命周期、GPU ordering 或平台 API 中不明显的原因。
- 不修改 `.rules`；如发现可复用陷阱，只在最终报告提出建议。

### 3.3 行为不变量 {#behavior-invariants}

- `request_layout → prepaint → paint → completed frame → submit` 顺序不变。
- cached prepaint/paint replay 在 atlas epoch 未变化时保持现有行为和性能。
- atlas epoch 变化时，当前窗口必须完整 repaint，不能 replay 旧 sprite tile。
- repeated present 在没有 atlas mutation 时继续复用同一 completed frame。
- current-frame working set 永远完整渲染；cache budget 不能产生缺字或缺图。
- `Entity`、focus、IME、input、accessibility 和 Task 语义不变。
- device recovery 使用同一 atlas epoch/full-refresh contract。
- macOS 不启用 LCD atlas；Windows/WGPU 的 subpixel 路径保持现状。
- Gamma/contrast ownership 明确，不能产生重复校正。

## 4. 目标类型和接口 {#target-types}

以下名称是实施目标。若编译或兼容性要求轻微调整，进度账本必须记录原因，不得改变
其职责边界。

### 4.1 Atlas domain {#atlas-domain}

把 atlas 类型从 `platform.rs` 移到 `atlas.rs`，仍从 crate root re-export。

```rust
pub enum AtlasContentKind {
    GlyphAlpha,
    GlyphSubpixel,
    GlyphColor,
    SvgMask,
    Image,
}

pub enum AtlasTextureKind {
    Monochrome,
    Polychrome,
    Subpixel,
}

pub enum AtlasBorderMode {
    Transparent,
    Extrude,
}

pub struct AtlasPolicy {
    // Internal page size, per-content retained byte budgets, and padding policy.
}

pub struct AtlasEpoch(u64);
pub struct AtlasFrameId(u64);
```

`AtlasTextureKind` 只表达 GPU format；`AtlasContentKind` 只表达 cache policy。

### 4.2 Common allocator and backend {#atlas-backend}

公共 core 拥有 page 和 etagere allocator：

```rust
struct AtlasPage {
    texture_id: AtlasTextureId,
    content_kind: AtlasContentKind,
    texture_kind: AtlasTextureKind,
    allocator: BucketedAtlasAllocator,
    resident_bytes: usize,
    last_used_frame: AtlasFrameId,
    entries: FxHashSet<TileId>,
}
```

backend trait 缩窄为 GPU 操作：

```rust
trait AtlasBackend {
    fn create_texture(&mut self, descriptor: AtlasTextureDescriptor) -> Result<()>;
    fn upload(&mut self, upload: AtlasUpload<'_>) -> Result<()>;
    fn destroy_texture(&mut self, texture_id: AtlasTextureId);
    fn clear_textures(&mut self);
}
```

backend 不再拥有 allocator、LRU、budget、live-key count 或 free-list policy。

### 4.3 Stable identity and retirement {#identity-retirement}

- `AtlasTextureId` 和 `TileId` 单调分配，在 atlas 实例生命周期内不得复用。
- etagere `AllocId` 只保存在内部 `AtlasEntry`；公开 `TileId` 不再由 `AllocId`
  派生。
- `remove` 只把 entry 移出 key lookup 并放入 retirement queue。
- retired allocation 成为 tombstone，不再单独回到 page allocator。
- 空间只通过整页 compaction 回收；replacement page 必须使用新的 texture ID。
- retired page 使用新的 texture ID 重建；不能把旧 ID 指向新 GPU texture。
- ID 计数溢出必须显式报错或安全清空并强制 full refresh，不能 wrap。

### 4.4 Frame integration {#frame-integration}

`Scene` 暴露本帧引用的 atlas tile/page usage。`Frame` 保存构建时的
`AtlasEpoch`。`Window::draw` 的顺序改为：

1. `sprite_atlas.begin_frame()` 应用 pending removal 和 budget maintenance；
2. 比较 atlas 当前 epoch 与 `rendered_frame.atlas_epoch`；
3. 不同则 `force_refresh()`，禁止 view cache replay；
4. 构建 next frame；
5. `sprite_atlas.finish_frame(next_frame.scene.atlas_usage())`；
6. 记录 next frame epoch；
7. swap frame 并 submit。

atlas mutation 不能在 frame build 中改变 epoch。frame build 期间到达的 remove 请求
排队到下一次 `begin_frame`。

### 4.5 Raster contract {#raster-contract}

平台 text system 返回明确 format：

```rust
pub struct GlyphRasterInfo {
    pub bounds: Bounds<DevicePixels>,
    pub format: GlyphRasterFormat,
}

pub struct RasterizedGlyph {
    pub info: GlyphRasterInfo,
    pub pixels: Vec<u8>,
}
```

`RenderGlyphParams::is_emoji` 在兼容期只作为 source/style hint，不能决定
`AtlasTextureKind`。`paint_glyph` 和 `paint_emoji` 的公开 API 可保留为 wrapper，内部
必须收敛到同一 paint path。

### 4.6 Budget semantics {#budget-semantics}

- budget 单位是实际 resident GPU texture bytes。
- 每个 `AtlasContentKind` 有独立 retained-cache budget。
- current completed-frame usage 构成 working-set floor。
- 单帧 miss 可临时超过 budget；在下个 frame boundary 压缩。
- 默认 policy 由实验确定，先使用 test-injected small budget 验证算法。
- oversized entry 使用 dedicated page，并纳入 working-set floor。
- production default 不先暴露为用户 setting。

## 5. 阶段总览 {#phase-overview}

| 阶段 | 交付结果                                          | 关键实验         |
| ---- | ------------------------------------------------- | ---------------- |
| 0    | 可复现 baseline、统计和进度账本                   | TEXT-001         |
| 1    | 正负坐标一致的 subpixel quantization              | TEXT-002         |
| 2    | atlas domain 抽离和公共 allocator                 | 行为保持 tests   |
| 3    | stable identity、epoch、retirement、replay safety | TEXT-003         |
| 4    | content-class budget、LRU、compaction             | TEXT-004/005/006 |
| 5    | format-aware glyph raster contract                | TEXT-007         |
| 6    | gutter 和 sampling correctness                    | TEXT-008         |
| 7    | bounded strike/raster-info cache                  | TEXT-009         |
| 8    | 数据驱动的 upload batching 决策                   | TEXT-010         |
| 9    | 全量验证、文档和移交                              | TEXT-011         |

## 6. 阶段 0：固定 baseline 和可观测性 {#phase-0}

### 工作 {#phase-0-work}

1. 记录 HEAD、工作树、Rust toolchain、target、GPU/driver、display scale 和字体。
2. 创建进度账本和 `.tmp/text-rendering-refactor/`。
3. 在不改变默认 release 行为的前提下增加 atlas snapshot：
   - page count；
   - resident bytes；
   - entry count；
   - hits/misses；
   - allocations；
   - pending uploads、upload calls 和 bytes；
   - removals、retirements、evictions、compactions；
   - current-frame working-set bytes；
   - budget-pressure frames。
4. 复用 `frame-diagnostics` 和 headless renderer，不创建第二套 profiler。
5. 建立固定 workload：
   - Latin、CJK、combining marks、ligatures、RTL；
   - OpenMoji；
   - SVG 和普通 image；
   - 14/16/20 px；
   - scale 1.0/1.25/2.0；
   - 长时间滚动和 theme/resize/scale change。
6. 运行 TEXT-001，原始数据放入 `.tmp`。

### 验证 {#phase-0-validation}

```sh
cargo check --locked -p gpui
cargo test --locked -p gpui
cargo test --locked -p gpui_wgpu --test headless_renderer
./script/clippy -p gpui
./script/clippy -p gpui_wgpu
```

### 退出条件 {#phase-0-exit}

- 所有统计有定义和单位；
- disabled diagnostics 不分配事件 buffer；
- baseline failure 已独立记录；
- workload 和随机 seed 固定。

## 7. 阶段 1：修复 subpixel quantization {#phase-1}

### 工作 {#phase-1-work}

1. 抽取纯函数，以整数 subpixel tick 表达量化结果：

   ```rust
   struct QuantizedGlyphCoordinate {
       integer: i32,
       variant: u8,
   }
   ```

2. 使用 `div_euclid` 和 `rem_euclid` 分解 tick。
3. x/y 共用算法，`SUBPIXEL_VARIANTS_Y = 1` 仍走同一路径。
4. `paint_emoji` 的整数对齐也复用同一 helper 或清楚说明差异。
5. 添加边界 tests：负值、正值、tie、整数、接近 tie、scale factor。
6. 添加窗口左边缘 clip/scroll headless pixel test。

### 退出条件 {#phase-1-exit}

- TEXT-002 通过；
- 不改变非负整数坐标输出；
- `RenderGlyphParams` 的 variant 始终在合法区间；
- 无额外 allocation。

## 8. 阶段 2：抽离 atlas domain 和公共 allocator {#phase-2}

### 2A. 文件和 API 边界 {#phase-2-domain}

1. 新建 `crates/gpui/src/atlas.rs`。
2. 从 `platform.rs` 移入 atlas key、tile、texture/content kind、policy、state、backend
   contract 和 headless atlas。
3. 在 `gpui.rs` re-export，保持现有 `gpui::AtlasTile` 等路径。
4. 这一提交只移动职责，不改变 runtime behavior。

### 2B. 公共 page allocator {#phase-2-allocator}

1. 把 `BucketedAtlasAllocator`、page size、entry metadata 和 monotonic ID allocation
   移入公共 core。
2. WGPU、Metal、DirectX backend 改为 texture storage 和 upload adapter。
3. 删除 `AtlasTextureList::free_list` 和 backend `live_atlas_keys`。
4. backend storage 使用 opaque monotonic `AtlasTextureId` 查找；不得按复用 slot 解释
   resource identity。
5. miss builder 改为 lock 外构建、lock 内 double-check insert。
6. 保持当前 1024×1024 default page behavior，budget 和 padding 暂不在本阶段开启。

### 测试 {#phase-2-tests}

- 同 key builder 只执行一次；
- 并发 double-check 不产生重复 resident entry；
- texture/tile ID 不复用；
- allocate、remove 和 empty-page destroy 在三个 backend contract tests 一致；
- oversized entry 正确受 max texture size 限制；
- device clear 后旧 ID 不会解析到新 texture。

### 退出条件 {#phase-2-exit}

- 三个 backend 不再拥有 allocator policy；
- 行为和 phase-0 pixel baseline 一致；
- atlas lock 不包围 raster builder；
- scoped check/test/clippy 通过。

## 9. 阶段 3：frame-aware identity 和 retirement {#phase-3}

### 工作 {#phase-3-work}

1. 增加 `AtlasEpoch`、`AtlasFrameId`、pending removal 和 retired entry/page queues。
2. `Scene` 收集 sprite 使用的 tile/page identity；clear/replay/insert 都维护 usage。
3. `Frame` 记录 atlas epoch；`BuiltFrame` 保持只读 projection。
4. 在 `Window::draw` 接入 begin/finish frame 顺序。
5. epoch mismatch 使用现有 `force_refresh`，禁止所有 cached view paint replay。
6. `remove` 不再立即 deallocate；它排队、推进 epoch 并请求窗口 refresh。
7. 新 completed frame 不再引用 retired page 后才 destroy page；单个 retired entry
   只形成 tombstone，不能让旧 region 被新 upload 就地覆盖。
8. device lost/atlas clear 也推进 epoch；删除 backend 特有的隐式 stale-tile 假设。
9. `Window::drop_image` 保持公开签名兼容，但不再静默丢弃错误，并保证 refresh。
10. 增加 debug assertion：frame build 中不能应用 invalidating retirement。

### 必须测试的序列 {#phase-3-sequences}

```text
paint A → complete frame → remove A → present old frame → full repaint
paint A → remove A → insert B same size → replay attempt
paint A → destroy last page → create new page → old scene lookup
device lost → clear atlas → forced render → cached view
remove image in another window → both windows refresh independently
```

### 退出条件 {#phase-3-exit}

- TEXT-003 通过；
- old Scene 最多继续显示 retired old content，绝不能显示 new unrelated content；
- epoch 不变时 cache replay count 与 baseline 一致；
- epoch 变化时 replay count 为零并完成 full repaint；
- repeated present 不触发不必要 rebuild。

## 10. 阶段 4：content-class budget、LRU 和 compaction {#phase-4}

### 工作 {#phase-4-work}

1. 引入 `AtlasContentKind`，保持 `AtlasTextureKind` 只表达 pixel format。
2. 给每类内容注入独立 test budget；production default 延后到实验后确定。
3. page 记录 resident bytes、last-used frame、live entries、used entries 和 occupancy。
4. `finish_frame` 从 Scene usage 更新 page recency 和 working-set floor。
5. miss 超过 retained budget 时允许当前 frame 完成，并记录 pressure。
6. 下一 `begin_frame` 按顺序处理：
   - 删除不在 completed frame usage 中的冷 entry/page；
   - LRU retire 完全冷 page；
   - 仍超预算时 retire sparse page，推进 epoch，full repaint 后把 hot entries 重排；
   - 达到 effective budget 或只剩 working set 时停止。
7. compaction 使用整页 retirement 和新 monotonic texture ID，不在旧 page 上就地覆盖。
8. explicit image lifecycle 和 automatic cache eviction 共享同一 retirement path。
9. 为 pressure、working-set floor 和 compaction reason 增加 diagnostics。

### 不变量 {#phase-4-invariants}

- 不为满足 budget 而返回空 glyph 或跳过 sprite；
- 当前 frame working set 超预算时记录事实，不循环 eviction/reinsert；
- 一个 content class 的 pressure 不直接驱逐其它 class；
- 单个 tile removal 不调用 allocator deallocate；只有整页 replacement 回收空间；
- compaction 失败时保留正确 resident content 并记录错误，不破坏 lookup；
- allocation error 传播到现有 `Result` 边界。

### 退出条件 {#phase-4-exit}

- TEXT-004、TEXT-005、TEXT-006 通过；
- production default budget 有实验依据和注释；
- 长时间 CJK scroll resident bytes 形成平台；
- 没有 eviction storm 或每帧 full refresh；
- 大 image 与 glyph cache 隔离。

## 11. 阶段 5：format-aware glyph raster contract {#phase-5}

### 工作 {#phase-5-work}

1. 增加 `GlyphRasterFormat`、`GlyphRasterInfo` 和 `RasterizedGlyph`。
2. 把 bounds 和 format 查询统一，避免“先由 shaping 猜 format，再由 rasterizer 产生
   不同像素”的双重真相。
3. `AtlasKey::Glyph` 包含实际 raster format 或等价稳定 key。
4. 合并 `Window::paint_glyph` 和 `paint_emoji` 的内部实现；公开方法保留 wrapper。
5. Windows 保留 per-glyph `TranslateColorGlyphRun` 权威路径。
6. WGPU 使用 Swash output content 决定 format；已知 font list 只作为 source hint。
7. macOS 删除 PostScript name 白名单。优先使用 CoreText color-glyph trait；若无法
   可靠获得 per-glyph format，对 color-capable font 采用保守 RGBA raster。
8. color glyph 禁止 synthetic bold/italic，除非平台实现和 tests 明确支持。
9. 明确 premultiplied/straight alpha 和 BGRA/RGBA contract；转换只能发生一次。
10. 增加 licensed COLRv1、OpenType-SVG、bitmap color 和 monochrome fixtures；更新
    license manifest。

### 兼容性 {#phase-5-compatibility}

- `ShapedGlyph::is_emoji` 暂时保留，避免公开 API 一次性破坏；
- 它不再是 texture kind 的权威来源；
- 如新增更准确字段，旧字段标注迁移意图，但本 Goal 不留下双重行为分支。

### 退出条件 {#phase-5-exit}

- TEXT-007 在可执行 backend 通过；
- 普通 glyph 仍可 tint；color glyph 不受 text color tint；
- 同一 color-capable font 中的 monochrome glyph 即使保守走 RGBA 也必须视觉正确；
- 三个平台 raster bytes contract 有单元测试。

## 12. 阶段 6：padding、sampling 和边界正确性 {#phase-6}

### 工作 {#phase-6-work}

1. 实现 inner content bounds 和 outer allocation bounds。
2. `AtlasTile::padding` 表示实际 gutter，shader UV 继续只看 inner bounds。
3. policy：
   - alpha/subpixel/color glyph：transparent gutter；
   - SVG mask：transparent gutter；
   - ordinary image：edge-extruded gutter。
4. padding helper 覆盖 1-byte 和 4-byte formats，并验证 row length/overflow。
5. cropped image sub-tile 只能修改 inner bounds，不能丢失 allocation identity。
6. WGPU/Metal 保留 linear sampler；DirectX address mode 改为 clamp。
7. 增加 integer/fractional/scale/rotation/adjacent-tile headless pixel matrix。

### 退出条件 {#phase-6-exit}

- TEXT-008 通过；
- 无 cross-tile bleed；
- ordinary image 边缘无透明 seam；
- glyph layout bounds 不因 padding 增大；
- resident-byte accounting 包含 padding 和真实 page bytes。

## 13. 阶段 7：有预算的 strike/raster-info cache {#phase-7}

### 工作 {#phase-7-work}

1. 将 flat `raster_bounds` map 替换为两级 cache：
   - `GlyphStrikeKey`；
   - strike 内 `PackedGlyphKey → GlyphRasterInfo`。
2. strike key 包含所有影响 bounds/coverage/format 的参数，不包含 paint-only color，除非
   平台 dilation 确实由颜色决定。
3. subpixel variant 使用紧凑、范围受验证的 key。
4. cache 同时限制 strike count 和 estimated bytes；LRU hit/update 在短 mutex 区间内。
5. platform raster query 在 lock 外执行，插入时 double-check。
6. 第一版不缓存 bitmap。atlas miss 需要 pixels 时重新 rasterize。
7. 如果 macOS dilation 仍依赖 foreground luminance，明确把离散 dilation level 放入
   strike key，不能错误复用。
8. 提供 test-only limit injection 和 snapshot。

### 退出条件 {#phase-7-exit}

- TEXT-009 通过；
- cache bytes/count 不超限；
- 同 strike lookup 不重复构造平台 font state；
- eviction 后结果与 cold raster 一致；
- 没有永久增长的 replacement map。

## 14. 阶段 8：upload batching 决策 {#phase-8}

这是唯一允许“实验后拒绝”的实现阶段，但实验本身必做。

### 实验顺序 {#phase-8-experiments}

1. 先测量 current per-entry uploads。
2. WGPU 试验单 staging buffer 和多 copy command，不增加 full-page CPU mirror。
3. 只有相邻 region 足够多时才试验 union/coalescing，并记录 byte amplification。
4. WGPU 达标后分别评估 Metal blit 和 DirectX upload；不能假设收益相同。
5. 若 TEXT-010 未达标，删除 prototype，并在进度账本记录 `REJECTED`。

### 保留门槛 {#phase-8-gate}

- upload calls 有实际下降；
- frame p95 不回退超过 5%；
- uploaded bytes 没有不可接受的放大；
- CPU resident memory 不增加一份无预算 atlas mirror；
- 代码没有让 backend contract 重新承担 allocator policy。

## 15. 阶段 9：收敛和全量验证 {#phase-9}

### 清理 {#phase-9-cleanup}

1. 删除旧 allocator/free-list/live-key code、名称白名单和 obsolete tests。
2. 删除迁移期 wrapper 之外的双路径；公开 wrapper 写明兼容边界。
3. 更新本报告、`text-layout.md`、rustdoc、progress ledger 和最终 architecture diagram。
4. 更新 evidence/experiments 的实际结果和最终 commit。
5. 检查没有默认网络、Skia dependency、telemetry 或 profiler overhead。

### 最终验证 {#phase-9-validation}

每次修改运行最窄相关命令。最终至少运行并记录：

```sh
cargo fmt --all -- --check
cargo test --workspace
./script/clippy
./script/check-philosophy
cd docs && npx prettier --check src/
```

当前 Linux 主机还应运行：

```sh
cargo test --locked -p gpui
cargo test --locked -p gpui_wgpu
cargo test --locked -p gpui_wgpu --test headless_renderer
```

平台验证矩阵：

| 平台                | 必做                                                              |
| ------------------- | ----------------------------------------------------------------- |
| Linux/WGPU hardware | unit、headless pixel、CJK budget、color fixture、smoke            |
| Linux/WGPU fallback | headless pixel、budget、device reset                              |
| macOS/Metal         | compile、native pixel matrix、CoreText color font、scale change   |
| Windows/DirectX     | compile、native pixel matrix、DirectWrite color font、device lost |

无法在当前主机运行的 native runtime 必须写出：命令、fixture、预期、采集 artifact 和
判定阈值，并标记 `NOT RUN`。不得把 cross-check 当作 native runtime PASS。

### 最终退出条件 {#phase-9-exit}

- TEXT-011 通过；
- 所有完成定义满足；
- 工作树只包含已解释、已验证的重构内容；
- progress ledger 没有未关闭的 blocker、TODO 或实验；
- architecture report 与代码一致；
- Goal 才可标记 complete。

## 16. 推荐提交序列 {#commit-sequence}

建议每项一个 signed commit：

1. `gpui: Record text atlas baseline diagnostics`
2. `gpui: Fix negative glyph subpixel quantization`
3. `gpui: Isolate the sprite atlas domain`
4. `gpui: Centralize atlas page allocation`
5. `gpui: Make atlas resource identities monotonic`
6. `gpui: Retire atlas entries at frame boundaries`
7. `gpui: Invalidate cached paint across atlas epochs`
8. `gpui: Add content-class atlas budgets`
9. `gpui: Compact sparse atlas pages`
10. `gpui: Return explicit glyph raster formats`
11. `gpui_macos: Detect color-capable fonts without name lists`
12. `gpui: Add atlas texture gutters`
13. `gpui: Bound the glyph raster-info cache`
14. 可选：`gpui_wgpu: Batch atlas uploads`
15. `docs: Complete text rendering refactor validation`

提交名称可以随实际 scope 调整，但不得把 identity、lifecycle、budget、raster format
和 sampling correctness 压成一个不可审查的大提交。
