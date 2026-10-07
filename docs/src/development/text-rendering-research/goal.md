---
title: Text Rendering Refactoring Goal
description: Copyable Goal mode instruction for completing the GPUI text rendering and atlas refactor.
---

# GPUI 文本渲染完整重构 Goal

> 执行状态（2026-10-07）：该 Goal 已在 `refactor/gpui-text-rendering` 执行到阶段 9。
> 实际提交、实验 artifact、跨平台 `NOT RUN` runbook 和最终验证见
> [进度账本](../text-rendering-refactor-progress.md)。以下文本保留为可复用的完整执行指令。

在仓库根目录开启新对话，将下面整段复制到 composer：

```text
/goal
在 /home/begonia/Documents/Github/Xero-Team/ZZZ 中完整完成 GPUI 文本渲染、glyph
raster cache 和 sprite atlas 生命周期重构。

开始任何修改前，完整阅读并遵守：
- 根 AGENTS.md
- .rules
- docs/AGENTS.md
- docs/src/development/ownership-and-data-flow.md
- docs/src/development/gui-framework-research/refactoring-plan.md
- docs/src/development/gpui-refactor-progress.md
- docs/src/development/text-layout.md
- docs/src/development/text-rendering-research/report.md
- docs/src/development/text-rendering-research/refactoring-plan.md
- docs/src/development/text-rendering-research/evidence.tsv
- docs/src/development/text-rendering-research/experiments.tsv
- .agents/skills/hunting-code-smells/SKILL.md 及其 REFERENCE.md

把 text-rendering-research/refactoring-plan.md 作为执行合同。按阶段 0 到阶段 9
持续自主工作，不要只修负坐标 bug、只做 atlas 抽象、只提交原型或完成一部分后停止。
阶段 8 的 upload batching 可以由实验决定保留或拒绝，但实验、清理和结论必须完成；
其它阶段均为必做。

开始时检查 git status 和当前 HEAD。计划基线是
a3a0f9734069b543f3fe1e0bdd77a37fbd1b2b31；若 HEAD 已前进，先审查目标 crate 的
差异并把新执行基线写入进度账本。保留所有用户修改，不得 reset、clean、checkout
丢弃或覆盖。若当前不在隔离分支，在代码修改前创建或切换到
refactor/gpui-text-rendering。不要 push、开 PR 或合并 main。

创建并持续维护
docs/src/development/text-rendering-refactor-progress.md，逐阶段记录 baseline、设计决定、
改动、实验、验证命令、PASS/FAIL/BLOCKED/NOT RUN、原始 artifact 路径、signed commit
SHA 和下一步。原始 benchmark、pixel image、trace 与临时 fixture 放在
.tmp/text-rendering-refactor/。每个提交只做一个可独立回退的逻辑变化，并使用
git commit -s；不得留下不编译的中间提交、无删除条件兼容层、死 feature、未说明 TODO
或双实现。

必须优先修复并证明两个 P0 不变量：
1. 使用整数 subpixel tick 加 div_euclid/rem_euclid 正确分解正负 glyph origin，完成
   TEXT-002。
2. 消除 cached Scene 保留旧 AtlasTile、tile/page 被回收重用后采样新内容的 ABA 风险，
   完成 TEXT-003。AtlasTextureId 和 TileId 在 atlas 生命周期内不得复用；remove 和
   eviction 必须先 retirement；retired suballocation 只能成为 tombstone，不能单独回到
   allocator；空间只能通过使用新 texture ID 的整页 compaction 回收；atlas epoch 变化
   必须使下一 completed frame 禁止 cached paint replay。

随后严格按计划完成：
- 把 atlas domain 从 platform.rs 移入单一逻辑组件 atlas.rs，同时保持 gpui root
  re-export 和公开 façade 兼容；
- 把 etagere page allocator、entry/page metadata、budget、LRU、retirement、compaction
  和 diagnostics 收敛为一份 gpui 公共实现；WGPU、Metal、DirectX backend 只保留 GPU
  texture create/upload/destroy/resource lookup；
- 将 atlas 接入现有 BuiltFrame、Scene replay、Window force_refresh 和 device recovery
  边界；frame build 中不能应用会改变 epoch 的 retirement；
- 分离 AtlasContentKind 与 AtlasTextureKind，按 GlyphAlpha、GlyphSubpixel、GlyphColor、
  SvgMask、Image 独立管理 retained-cache budget；
- budget 以 resident GPU page bytes 计算，以当前 completed-frame working set 为下限，
  允许单帧临时超预算并在后续 frame compaction，不得通过丢 glyph/image 满足预算；
- 引入 GlyphRasterFormat、GlyphRasterInfo、RasterizedGlyph，由实际 raster result 决定
  Alpha8/SubpixelBgra8/ColorBgra8；is_emoji 只保留为兼容 hint，不得继续决定 atlas
  format；
- Windows 保留 per-glyph DirectWrite color detection；WGPU 使用 Swash output content；
  macOS 删除 AppleColorEmoji PostScript name 白名单，至少使用 CoreText color-glyph
  capability 并对 color-capable font 采用视觉正确的保守 RGBA 路径；
- 实现一像素 atlas gutter：glyph/SVG 使用 transparent border，ordinary image 使用
  edge extrusion；tile.bounds 表示 inner content；DirectX sampler address mode 改为
  clamp；完成 TEXT-008；
- 用 GlyphStrikeKey + PackedGlyphKey 替换无限增长的 flat raster_bounds map，同时限制
  strike count 和 estimated bytes；第一版不要缓存第二份 bitmap，除非独立实验支持；
- 完成 TEXT-010 后决定 upload batching。未达阈值则删除 prototype 并记录 REJECTED，
  不得为“像 Skia”保留常驻 full-page CPU mirror。

不要添加 Skia 或 rust-skia production dependency，不要增加默认网络下载。Skia checkout
只作为只读设计证据；rust-skia 只允许在 .tmp 中作为可选 metrics/pixel oracle。若复制
任何上游代码，记录 repository、commit、path 和 license，并做最小 Rust/GPUI 适配。

始终保持 request_layout→prepaint→paint→completed frame→submit、Entity/Context、Task
drop cancellation、focus/input/IME/accessibility 和公开 Window 行为兼容。不要为了 batching
重新排序可能重叠的透明 sprite。不要让 font rasterization 或 image copy 在 atlas mutex
内执行。所有 fallible operation 必须传播或记录错误；不新增 unwrap，不静默丢弃 Result。

每次修改运行最窄相关 check/test。使用现有 real headless renderer 建立 negative
subpixel、ABA、CJK budget、working-set-over-budget、content-class isolation、color font、
gutter 和 device recovery tests。严格执行 experiments.tsv 的阈值；不得用单个平台结果
冒充跨平台结论。

最终必须运行并记录：
- cargo fmt --all -- --check
- cargo test --workspace
- ./script/clippy
- ./script/check-philosophy
- cd docs && npx prettier --check src/
- cargo test --locked -p gpui
- cargo test --locked -p gpui_wgpu
- cargo test --locked -p gpui_wgpu --test headless_renderer

在可用主机上完成 Linux/WGPU hardware 和 fallback runtime 验证。macOS/Metal、
Windows/DirectX 或对应 native color-font/device-lost tests 无法在当前主机运行时，写出
精确 runbook，包含命令、fixture、预期、artifact 和判定阈值，并标记 NOT RUN；不得
声称通过。

持续自主推进。普通编译错误、测试回归、borrow checker 问题、平台 cfg、设计取舍和
实验失败由你调查、修复或按合同回退。只有缺少必须由我提供的产品决策、凭证、专用
硬件或外部平台结果时才提问。达到 refactoring-plan.md 的全部完成定义、更新最终架构
文档和进度账本、清除所有临时重构残留后，才把 Goal 标记 complete。
```
