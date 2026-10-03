---
title: GPUI Refactoring Execution Plan
description: Staged execution contract for refactoring ZZZ's GPUI infrastructure.
---

# GPUI 完整重构执行计划

本文把[架构研究报告](./report.md)中的方向转换为可以由一个长期 Goal
连续执行、逐阶段验收和随时恢复的实施计划。研究报告解释为什么这样改；本文规定
按什么顺序改、每一阶段交付什么、什么结果允许进入下一阶段。

- 计划基线：ZZZ `152a5eb983a883c69a6cc4eae082312ba75aa9f9`
- 参考仓库：`.tmp/ui_ref/` 下的 9 个只读 shallow checkout
- 实验阈值：[experiments.tsv](./experiments.tsv)
- 可复制的 Goal：[goal.md](./goal.md)

如果执行开始时 ZZZ HEAD 已变化，先确认新增提交是否修改本计划覆盖的 crate。只要
现有工作没有被覆盖，就把新 HEAD 记录为执行基线并继续；不得重置或丢弃已有工作。

## 1. 目标与完成定义 {#goal-and-done}

重构完成后，ZZZ 继续通过 `gpui` 暴露稳定 façade，但内部具备明确的 runtime、
frame、interaction、text input、accessibility、render 和 platform 所有权边界。
应用代码不需要集体迁移，现有编辑、输入、渲染和窗口行为保持一致。

只有同时满足以下条件，整个 Goal 才能标记完成：

1. 阶段 0 至阶段 7 的必做项全部完成；阶段 8 根据实验得到“实施”或“拒绝”的明确
   结论。
2. `Window` 不再直接实现 frame scheduling、interaction routing、IME bridge、
   accessibility tree 和 renderer submission 的全部算法；它作为公开 façade 组合
   内部 owner。
3. renderer contract 不依赖 `App`、`Entity` 或公开 `Window`；平台 backend 通过
   compatibility adapter 使用统一 frame 输出。
4. 每个平台公开 capability 状态；不支持的能力不再依靠静默 no-op 表达。
5. 公开 GPUI API、Entity/Context 语义、Element 三阶段、Task 取消语义、Editor
   IME/focus/key dispatch 行为保持兼容。经过评审的 deprecated adapter 可以保留一
   个迁移周期。
6. 必要的性能、allocation、pixel、semantic、input、IME 和并发验证达到本文阈值；
   未能在当前主机运行的 macOS/Windows/native screen-reader 检查有可执行 runbook，
   并明确标记为 `NOT RUN`。
7. `cargo test --workspace`、`./script/clippy`、`./script/check-philosophy` 和文档格式
   检查通过，或只剩经过复现并记录的既有 baseline failure。
8. 进度账本记录所有阶段、实验、决策、提交和剩余平台 QA；没有未说明的 TODO、
   临时 feature、双实现或死 compatibility path。

## 2. 范围 {#scope}

核心范围：

- `crates/gpui`
- `crates/gpui_platform`
- `crates/gpui_linux`
- `crates/gpui_macos`
- `crates/gpui_windows`
- `crates/gpui_web`
- `crates/gpui_wgpu`
- `crates/gpui_tokio`

只有在边界迁移需要时才修改：

- `crates/editor`
- `crates/ui`
- `crates/ui_input`
- `crates/ui_prompt`
- `crates/component`
- `crates/component_preview`
- `crates/workspace`
- `crates/zzz`

不属于本次重构目标：

- 更换 GPUI、引入第二套 runtime 或完整 property/binding system；
- 把 `gpui-rsx` 设为默认 UI API；
- 将移动端支持放入桌面重构 critical path；
- 公开 backend-specific custom draw registry；
- 顺手重写业务 UI、主题系统或产品功能；
- 为了与 Zed 对齐而恢复 ZZZ 已删除的账号、遥测、协作或原生 Agent 表面。

## 3. 执行规则 {#execution-rules}

### 3.1 工作区与提交 {#workspace-and-commits}

1. 开始代码修改前确认工作树状态。保留用户已有修改，不得 reset、clean 或覆盖。
2. 在隔离 worktree 或专用分支 `refactor/gpui-architecture` 中工作。不要 push、开 PR
   或合并到 `main`。
3. 创建 `docs/src/development/gpui-refactor-progress.md` 作为执行账本。每一阶段记录：
   baseline、改动、实验结果、验证命令、PASS/FAIL/BLOCKED/NOT RUN、提交 SHA 和下一
   步。
4. 原始 benchmark、pixel output 和临时分析放在 `.tmp/gpui-refactor/`；只把摘要、
   可复现命令和决策写入 tracked 文档。
5. 每个提交只包含一个可独立回退的行为或结构变化，并使用 `git commit -s`。阶段
   结束前不留下不编译的中间提交。
6. 参考仓库保持只读。移植代码时记录 source repository、commit、文件和 license。

### 3.2 上游处理 {#upstream-handling}

任何来自 Zed 的实现必须遵循 `absorbing-upstream` 的 A/B/C 流程：

- 先读取当前最新 upstream sync 报告，确定 general reviewed baseline；
- 对 frame diagnostics、`ThreadedDispatcher`、AccessKit core 和平台 adapter 的相关
  commits 逐个分类；
- 完整且安全的改动可归 A；需要适配 ZZZ 架构的最小移植归 B；依赖缺失
  AccessKit writer、账号、遥测、协作或其它被删除架构的改动归 C；
- 不增加 named `upstream` remote，不用 `script/cherry-pick`；
- B 类必须记录 `Upstream`、`Retained`、`Omitted`，并在提交前通过 touched-crate
  check/test。

AccessKit 在本计划中是目标能力，不是预先批准的整块 cherry-pick。框架语义树、
action routing 和原生 adapter 应分别审查和移植。

### 3.3 每次修改必须保持的契约 {#invariants}

- `Entity::update` 不允许重入同一 entity；内部闭包继续使用传入的 `cx`。
- dropped `Task` 继续取消；新 spawn 必须 await、detach 或持有。
- `request_layout → prepaint → paint` 顺序和 cached range replay 正确性不变。
- focus、pointer capture、capture/bubble dispatch、keymap/action precedence 不变。
- IME 保持 UTF-16 range、marked text、candidate bounds、undo grouping 和 multi-cursor
  行为。
- platform event callback 不直接重入 entity update；所有主线程回写经过现有 context
  边界。
- 不新增 `unwrap()`，不对 fallible operation 使用 `let _ =`；遵守 `.rules`、
  `clippy.toml` 和 i18n 要求。
- 默认构建不启用 profiler 或 benchmark 开销。

## 4. 阶段总览 {#phase-overview}

| 阶段 | 结果                             | 进入条件                      | 退出条件                            |
| ---- | -------------------------------- | ----------------------------- | ----------------------------------- |
| 0    | 固定 baseline 与执行账本         | 当前工作树可安全继续          | baseline 可复现，实验命令可运行     |
| 1    | frame diagnostics 与性能预算     | 阶段 0                        | EXP-001/002/011 可采样，开销达标    |
| 2    | 并发与 accessibility 验证边界    | 阶段 1                        | EXP-006/008 通过或有明确平台欠账    |
| 3    | 跨平台真实 headless renderer     | 阶段 1                        | EXP-003 通过，或明确拒绝并保留现状  |
| 4    | `Window` 内部 owner 拆分         | 阶段 1/2，阶段 3 最好已完成   | façade 兼容，行为与预算不回退       |
| 5    | render contract 与 WGPU 模块化   | 阶段 3/4                      | EXP-004/005 通过，无依赖环          |
| 6    | platform capability 与 lifecycle | 阶段 4/5                      | backend matrix 可测试，无静默 no-op |
| 7    | UI 集成边界清理                  | text input/platform seam 稳定 | EXP-009/010 通过                    |
| 8    | 数据驱动的 invalidation 实验     | 阶段 1/4                      | EXP-007 决定实施或拒绝              |
| 9    | 收敛、全量验证与移交             | 前述阶段完成                  | 完成定义全部满足                    |

## 5. 阶段 0：固定 baseline {#phase-0}

### 工作 {#phase-0-work}

1. 记录 `git status`、HEAD、Rust toolchain、target、GPU/driver、字体、窗口 scale 和
   当前 upstream reviewed baseline。
2. 运行并记录：
   - `cargo check --locked -p gpui`
   - `cargo test --locked -p gpui`
   - Editor 的现有 IME、focus、input 和 teardown 相关 tests
   - `cargo check --locked -p gpui_platform -p gpui_wgpu -p gpui_linux`
3. 建立固定 workload：100k 行 Rust 文件输入/滚动、窗口 resize、command palette、
   tabs、settings、text/emoji/SVG/image/clip/shadow/path scene corpus。
4. 记录 clean/incremental check time、binary size、稳态 allocation、input latency 和
   当前 pixel/semantic 输出。无法测量的指标先标为 `MISSING BASELINE`，阶段 1 必须
   补齐。

### 退出条件 {#phase-0-exit}

- 账本中每条 baseline 都有命令、环境、结果和原始数据路径；
- 现有失败已从 clean baseline 复现，后续不会被误判为重构回归；
- workload 和随机 seed 固定。

## 6. 阶段 1：frame diagnostics 与预算 {#phase-1}

### 工作 {#phase-1-work}

1. 在现有 `profiler`、`InputLatencyTracker` 和 `BenchAppContext` 上增加 feature-gated
   frame journal。
2. 为 invalidation、draw start/end、layout、prepaint、paint、cache replay、present、
   skipped/coalesced frame 分配同一个 `FrameBuildId`。
3. 记录 dirty source、dirty reason、entity/window、cache hit 和 input provenance。
4. 增加外部 collector/snapshot API，默认 feature 下不分配 histogram 或事件 buffer。
5. 建立 EXP-001、EXP-002、EXP-011 的可重复 runner。

### 退出条件 {#phase-1-exit}

- instrumentation overhead 不超过 2%；
- no-op/cached frame allocation 不高于 baseline；
- 可以从一次 input 追踪到 invalidation、draw 和 present；
- disabled feature 的 release behavior 与 binary surface 不变。

## 7. 阶段 2：并发与 accessibility 边界 {#phase-2}

### 2A. `ThreadedDispatcher` {#phase-2-threaded-dispatcher}

1. 按 A/B/C 流程审查 Zed 实现。
2. 保留 `TestDispatcher` 的 virtual clock；新增 dispatcher 只用于 benchmark 和
   integration tests。
3. 覆盖 background-to-main handoff、timer、取消、panic cleanup、window teardown 和
   100 个固定随机 seed。
4. 通过 EXP-008 后，允许 `BenchAppContext` 选择 deterministic 或 threaded mode。

### 2B. AccessKit {#phase-2-accesskit}

1. 先引入 GPUI 内部 semantic node、stable element ID、action map 和 per-frame tree
   update；不先接平台 adapter。
2. 为 button、input、editor、list/tree、tabs、dialog、status notification 建立
   deterministic semantic snapshots 和 action tests。
3. 分别接 macOS、Windows、Wayland adapter。每个平台独立提交、独立回退。
4. 键盘 focus 和 accessibility focus 使用同一映射；action 只能触发一次。
5. 当前主机不能运行的 VoiceOver、Narrator、Orca 检查写入 runbook，并保留
   `NOT RUN`，不得用 snapshot 冒充 native QA。

### 退出条件 {#phase-2-exit}

- EXP-006、EXP-008 达到阈值；
- accessibility 关闭时没有 runtime overhead；
- 原 input/focus/IME tests 全部保持通过。

## 8. 阶段 3：真实 headless renderer {#phase-3}

### 工作 {#phase-3-work}

1. 从 gpui-ce 的 surface-free WGPU headless path 提取设计，不复制其 runtime 反向
   依赖。
2. 让普通 `Scene` 通过 offscreen texture 渲染并读回 RGBA；复用阶段 0 的 scene
   corpus。
3. 在 Linux software/hardware adapter 上跑 100 次；为 Windows CI 准备同一 runner。
4. pixel baseline 按平台、adapter、scale、font 固定；允许差异必须有批准记录。

### 决策门 {#phase-3-gate}

- 满足 EXP-003：接入 `test-support`，作为后续 renderer/Window 迁移的回归门。
- 不满足：保留 macOS visual path，把 WGPU headless 标为 rejected experiment；不得为
  通过 CI 而放宽到无法发现真实回归的阈值。

## 9. 阶段 4：拆分 `Window` {#phase-4}

按以下顺序做小提交。每一步先移动状态和私有函数，再建立 owner；不要在同一提交
同时改行为。

### 4A. Frame owner {#phase-4-frame}

- 新建内部 `frame/`：`FrameScheduler`、`WindowInvalidator`、dirty state、cache
  ranges、frame journal。
- `Window` 保留公开 invalidate/draw/present 方法，只委托 frame owner。

### 4B. Interaction owner {#phase-4-interaction}

- 新建 `interaction/`：hitboxes、dispatch tree、focus/tab、pointer capture、cursor、
  key/action capture/bubble。
- 用 snapshot tests 固定每类 event 的 routing order。

### 4C. Text input owner {#phase-4-text-input}

- 新建 `text_input/`：窄 `TextInputClient`、entity adapter、marked/selected range、
  candidate geometry。
- 平台层不再持有 `AsyncWindowContext` 语义，只通过 client capability 调用。

### 4D. Built frame {#phase-4-built-frame}

- frame build 完成后产生不可变 `BuiltFrame`：`RenderScene`、
  `InteractionSnapshot`、`TextInputSnapshot`、`AccessibilityUpdate`、
  `FrameDiagnostics`。
- renderer 和 platform adapter 只读 completed frame；cache replay 只从上一个
  completed frame 复制。

### 退出条件 {#phase-4-exit}

- 公开 consumer 不需要修改；必要迁移只发生在 GPUI/platform 内部；
- scene、semantic、input、focus、IME 和 visual tests 一致；
- EXP-001/002/011 不超过既定回归预算；
- `window.rs` 只保留 façade、组合与生命周期协调，不再保存可归属给上述 owner 的
  平行状态集合。

## 10. 阶段 5：render contract 与 WGPU 模块化 {#phase-5}

### 工作 {#phase-5-work}

1. 先在 `gpui` 内建立 `render_api/`，定义 backend-neutral 的 `RenderScene`、
   resource/atlas contract、`Renderer`、`RenderTarget` 和 `FrameSubmission`。
2. 保留 `PlatformWindow::draw` compatibility adapter，让现有 backend 逐个迁移。
3. 把 `gpui_wgpu/src/wgpu_renderer.rs` 拆为 resources、pipelines、frame、surface、
   drawing、headless；拆分本身不得改变公开 API。
4. 依次迁移 WGPU、Metal、DirectX。backend 能力差异通过 capability/result 表达，
   不向应用公开 downcast。
5. 只有 EXP-004/005 证明无 cycle、无 consumer edit、build/binary 预算达标，才把
   `render_api` 抽成 `gpui_render` crate。否则长期保留为 `gpui` 内部模块。

### 退出条件 {#phase-5-exit}

- renderer contract 不依赖 `App`、`Entity`、公开 `Window`；
- clean build 不超过 baseline 1.10，binary size 不超过 baseline 1.03；
- golden scene/pixel、surface loss/recovery 和 headless tests 通过。

## 11. 阶段 6：platform capability 与 lifecycle {#phase-6}

### 工作 {#phase-6-work}

把宽 `Platform`/`PlatformWindow` 内部拆为：

- `AppLifecycle`
- `WindowHost`
- `InputSource`
- `TextInputBridge`
- `AccessibilityBridge`
- `SystemServices`
- `RendererFactory` / `RenderTarget`

旧 trait 继续作为 composite façade，逐项委托。每个 backend 提供静态或运行时
`PlatformCapabilities`，并为 unsupported 能力返回可查询状态或明确 error。

重点验证：

- desktop window lifecycle 和 frame callback；
- Web 的 prompt、clipboard、IME position 和 window controls 不再静默成功；
- `run_embedded` 与外部 event loop 生命周期仍可表达；
- platform-only 改动不会扩大 rebuild graph。

### 退出条件 {#phase-6-exit}

- 每个 backend 有 capability matrix test；
- 默认 desktop API 兼容；
- 不存在已知 silent no-op；
- EXP-004 重新运行仍达标。

## 12. 阶段 7：UI 集成边界 {#phase-7}

### 工作 {#phase-7-work}

1. `ui_input` 移除 process-global `OnceLock` editor factory，改为显式
   `TextInputClient`/adapter 注册或构造参数。
2. `ui_prompt` 的 product-specific workspace 集成迁到 `zzz` 或 workspace UI
   integration；generic prompt renderer contract 留在 GPUI/UI。
3. 合并或重命名 `component` 与 `component_preview` 的 preview/registry 职责。
4. 新组件采用 semantic behavior 与 visual projection 分离；不批量重写旧组件。
5. 保留一版 deprecated adapter 时，必须有删除条件和 call-site 清单。

### 退出条件 {#phase-7-exit}

- EXP-009、EXP-010 通过；
- 初始化顺序不再导致 panic；
- Editor/Workspace 行为不变；
- editor-only 修改不再无必要地重编译 generic UI 层。

## 13. 阶段 8：invalidation 实验 {#phase-8}

此阶段由数据决定是否进入产品代码。

1. 先只记录 `Layout`、`Prepaint`、`Paint`、`Accessibility` dirty reason，不改变
   `cx.notify()` 的完整 invalidation 语义。
2. 在 10 个高频组件上内部试验 `notify_with(InvalidationScope)`。
3. 对比处理 node/view 数、phase time、pixel、hitbox、focus、IME 和 accessibility
   tree。

只有 phase work 至少下降 20%，且没有任何行为差异，才保留 scoped invalidation。
否则删除 scope API，把 dirty reason 仅作为 diagnostics 留下。这个“拒绝”也是阶段
完成，不允许为了追求目标架构而忽略实验结果。

## 14. 阶段 9：收敛与最终验证 {#phase-9}

1. 删除已无 caller 的 compatibility adapter、feature 和旧 owner；运行依赖/死代码
   检查。
2. 更新 GPUI architecture 文档、platform capability matrix、benchmark 使用说明和
   native QA runbook。
3. 复跑所有 EXP-001 至 EXP-011；EXP-012 仍是独立可选 RSX spike，不是完成条件。
4. 运行最终检查：

```sh
cargo fmt --all -- --check
cargo test --workspace
./script/clippy
./script/check-philosophy
cd docs && npx prettier --check src/
```

5. 在当前主机运行所有可用的 Linux/headless/manual smoke；macOS、Windows 和 native
   screen reader 检查没有执行时，列出 exact command、环境和预期结果。
6. 重读 public diff，确认没有产品功能、遥测、账号、网络默认值或 i18n 漂移。

## 15. 持续验证矩阵 {#continuous-validation}

每个小提交至少运行格式检查、touched crate `cargo check` 和最窄相关 test。每个阶段
结束运行下表中的对应检查。

| 变化                 | 最低验证                                                         |
| -------------------- | ---------------------------------------------------------------- |
| `gpui` runtime/frame | `cargo check/test --locked -p gpui`；EXP-001/002/011             |
| input/focus/IME      | GPUI input tests；Editor IME/multi-cursor tests；EXP-009         |
| renderer/WGPU        | `cargo check/test -p gpui_wgpu`；scene corpus；EXP-003/005       |
| platform             | touched platform crate check；capability tests；host smoke       |
| accessibility        | semantic snapshots；action/focus tests；native QA 状态           |
| `ui_*` boundary      | touched crate check/test；workspace/editor startup smoke         |
| upstream port        | A/B/C report、DCO commit、touched crate check/test、ledger check |
| docs                 | 对修改文件执行 Prettier write/check                              |

不要在一个失败后继续堆叠后续阶段。先把失败归类为 regression、baseline failure、
环境限制或实验拒绝，再记录和处理。

## 16. 完成报告 {#completion-report}

最终报告应包含：

- 目标架构与最终 crate/module 图；
- 每阶段提交列表和 A/B/C upstream 来源；
- public API 变化和 compatibility 状态；
- baseline/最终 latency、phase、allocation、compile、binary、pixel 数据；
- Linux/macOS/Windows/Web capability matrix；
- PASS/FAIL/BLOCKED/NOT RUN 验证表；
- rejected experiments 及原因；
- 剩余风险只允许是明确的外部平台 QA，不得留下未完成的核心代码迁移。
