---
title: GPUI Qt Base Gap Completion Plan
description:
  Staged implementation contract for closing the high-value gaps found
  by the GPUI and Qt Base comparison.
---

# GPUI Qt Base 差距闭环执行计划 {#gpui-qtbase-completion-plan}

本文是从 [GPUI 与 Qt Base 差距调研](./qtbase-gap-report.md) 导出的实施合同。它承接
已经完成的基础设施重构，不重做 `refactoring-plan.md` 的阶段 0–9。目标是在正确性、
可验证性、平台一致性和长期代码整洁优先的前提下，完善 GPUI 及其紧邻的通用 UI 层。

> **Rule:** 可以做必要的跨 crate 重构和破坏性内部 API 迁移。不能为了小 diff、
> 旧实现、表面兼容或“尽快通过测试”牺牲 ownership、frame lifecycle、IME、a11y、
> platform capability 或 ZZZ 哲学边界的正确性。

## 1. 完成定义 {#definition-of-done}

本计划只有在全部下列条件成立时才完成：

1. Tier-1 desktop support matrix 由源码、测试和 native runtime 结果共同生成或验证；
   不可运行项有精确 runbook 与 `NOT RUN` 原因。
2. `gpui` 的 semantic tree、action routing、focus、selection、overlay、cached replay、
   virtualized content、window teardown 和 IME 交互都有确定性 regression test。
3. ZZZ `ui` 中的交互组件按审计结果拥有正确的可访问 name、role、state、value、
   relation、keyboard focus 和 action；装饰元素不会制造噪声语义。
4. 通用 model/selection/focus/virtualization 协议已经用至少一个 tree 和一个 table/list
   消费者验证，或有数据支持的拒绝决定。不存在无消费者的框架抽象。
5. README、crate docs、示例和迁移说明与支持矩阵一致；公开 API 有明确 ownership、
   capability、feature gate 和升级语义。
6. GPU extension 仅在真实用例通过 RFC gate 时以窄接口落地；否则有记录的拒绝结论。
7. 每个阶段的改动、验证、失败、平台限制、提交 SHA 和下一步记录在新的 progress
   ledger；没有未解释的 `TODO`、双实现或临时代码。
8. 最终验证通过，或只保留清楚隔离、可复现且不由当前主机解决的 `NOT RUN` 项。

## 2. 范围和边界 {#scope-and-boundaries}

### 2.1 允许修改的范围 {#allowed-scope}

- `crates/gpui`：Element 语义、Window lifecycle、focus/input、capability contract、
  test support、公开文档和 examples。
- `crates/gpui_platform`、`gpui_macos`、`gpui_windows`、`gpui_linux`、`gpui_wgpu`、
  `gpui_web`：平台 adapter、capability truthfulness、runtime test hooks 和 renderer
  extension experiment。
- `crates/ui`：可复用的无头协议、基础交互组件、主题无关的组件 contract、a11y 和
  model/view migration。
- 紧邻的 `editor` 或产品组件：只有在验证 selection、IME、semantic text 或迁移
  消费者所必需时修改。
- `docs/src/development/gui-framework-research/`、GPUI README/docs/examples、测试和
  CI/脚本：用于生成支持矩阵、runbook、进度和验证。

### 2.2 禁止的范围膨胀 {#scope-exclusions}

- 不重写或复制 Qt Core、Network、SQL、XML、DBus、PrintSupport、QML 或 Qt Widgets。
- 不恢复账号、遥测、协作、托管文档、默认网络或其他被 ZZZ 删除的商业表面。
- 不为没有真实消费者的移动、嵌入式、动态 ABI 或 GPU RHI 建立永久 API。
- 不把 editor 的 buffer、LSP、i18n 或 extension-host 状态迁入 `gpui`。
- 不将 `cfg!(feature)`、cross-compile 或 mock adapter 当成 native runtime 通过。

### 2.3 目标分层 {#target-layering}

```text
gpui
  Element / Window / focus / input / layout / Scene / semantic tree
  Platform traits and test contracts

gpui_platform + OS crates
  concrete lifecycle, IME, accessibility, clipboard, DnD, renderer adapters

ui (or a proven future generic component crate)
  visual components + headless model/selection/navigation/a11y contracts

editor and product crates
  document model, editor semantics, ZZZ product policy, i18n, extensions
```

任何新 type 都必须先回答它属于哪一层、谁拥有生命周期、谁能观察/修改它、如何测试、
谁是第二个消费者。答不出来时，先做实验而不是提交抽象。

## 3. 执行规则 {#execution-rules}

### 3.1 开始与工作区 {#workspace-rules}

1. 先读取根 `AGENTS.md`、`.rules`、`docs/AGENTS.md`、本计划、差距报告和旧的
   `gpui-refactor-progress.md`。
2. 记录当前 HEAD、branch、working tree 和覆盖 crate 的现有改动。保留用户改动；
   禁止 `reset --hard`、`checkout --`、`clean` 或覆盖未知文件。
3. 若需要隔离，创建专用分支或 worktree，但不 push、不开 PR、不合并 main，除非用户
   另行要求。
4. 创建并维护
   `docs/src/development/gui-framework-research/qtbase-completion-progress.md`。每阶段
   记录 baseline、设计决策、改动、验证、失败、`NOT RUN`、提交 SHA 和 next action。
5. 原始 benchmark、screen shot、semantic dump、adapter logs 和临时 runner 放在
   `.tmp/gpui-qtbase-completion/`，不作为永久源码接口。

### 3.2 正确性不变量 {#invariants}

所有阶段必须保持：

- `Entity`/`Context` ownership、借用和 subscription 生命周期正确；不得在 entity
  update closure 中重新进入 entity update。
- `request_layout → prepaint → paint` 的相位顺序、cached replay、hit testing、focus、
  key dispatch、pointer dispatch 和 task drop-cancellation 行为正确。
- Text input 保持 UTF-16 range、marked text、多 cursor、IME candidate bounds 和
  platform-specific composition 语义。
- accessibility node identity 在单个 window 生命周期内稳定；action 只能路由到仍然
  有效的 frame/node，teardown 不留回调或 native adapter。
- `PlatformCapabilities` 不将未初始化、feature-disabled 或降级实现宣传为可用。
- 新用户可见字符串遵守 i18n 规则；英文 fallback 与 locale catalog 同步。
- 不新增哲学禁止的默认网络、账户、telemetry 或 hosted dependency。

### 3.3 提交与验证纪律 {#commit-and-validation-rules}

- 一个提交只含一个可回退的逻辑改变；提交使用 `git commit -s`。
- 先写会在旧实现失败的 regression test，再修改生产代码；无法这样做时说明原因。
- 每次修改至少跑最窄 `cargo check`、targeted test 和格式检查；不要把失败隐藏在
  未运行的全 workspace 测试之后。
- 完成一个阶段时运行该阶段完整 matrix、`./script/clippy` 的相关 crate、
  `script/check-philosophy`、`git diff --check` 和文档 Prettier。
- 真正的 baseline failure 必须记录命令、退出码、相关测试名和首次观察修订；不把它
  归因给新改动，也不说成通过。

## 4. 阶段总览 {#phase-overview}

| 阶段 | 名称                                              | 类型     | 主要 gate                                 |
| ---- | ------------------------------------------------- | -------- | ----------------------------------------- |
| 0    | Baseline、support matrix 与文档事实               | 必做     | 真实/未知 capability 分离                 |
| 1    | Semantic tree 与 native accessibility correctness | 必做     | 所有 semantic/action lifecycle tests 通过 |
| 2    | UI semantic audit 与交互组件 contract             | 必做     | 高风险组件有可测语义和 keyboard 行为      |
| 3    | Model、selection、focus、virtualization 协议      | 必做实验 | tree 和 table/list 双消费者或拒绝         |
| 4    | Text、IME 与 accessible editor boundary           | 必做     | CJK/selection/a11y text contracts 不回归  |
| 5    | Platform capability 和 desktop QA 收敛            | 必做     | Tier-1 matrix 有证据，不伪报支持          |
| 6    | API、examples、docs 与升级纪律                    | 必做     | docs 与实现同步，例子可编译               |
| 7    | Public GPU extension RFC/vertical slice           | 条件     | 两个真实消费者和跨后端 gate               |
| 8    | 收敛、性能和最终验收                              | 必做     | 完成定义与最终 matrix                     |

阶段可在不共享修改同一 ownership boundary 时并行研究，但代码落地顺序必须先有
阶段 0 的事实基线，再有 1/2/4 的 correctness，然后才是 3/7 的抽象和扩展。

## 5. 阶段 0：Baseline、support matrix 与文档事实 {#phase-0}

### 工作 {#phase-0-work}

1. 审计每个 `PlatformCapabilities` 字段、feature gate 和 backend 实现，生成
   capability inventory：`supported`、`unsupported`、`degraded`、`not-tested` 与
   证据位置。若 bool 无法表达重要降级，先设计兼容迁移，而不是立即全局替换为 enum。
2. 比较 `crates/gpui/README.md`、examples、crate docs 与实际 target cfg；修复
   macOS/Linux-only 等过时断言。写清 Tier-1、Tier-2、experimental 和 unavailable。
3. 把最小 desktop acceptance app 固定为版本化 fixture：多窗口、DPI、文本输入、
   CJK composition、candidate bounds、clipboard text/HTML/image、file dialog、DND、
   keyboard layout、focus、GPU recovery 和 accessibility action。
4. 建立可机读 support-matrix source，优先由 tests/feature config 生成文档表，而不是
   手抄多份 Markdown。无法自动推导的 runtime 项记录 runbook 链接。
5. 复核旧重构账本中 macOS/Windows `NOT RUN` 项，不重复已通过的 Linux tests。

### 退出条件 {#phase-0-exit}

- 每个 capability 有 owner、backend、feature、test/runbook 和状态。
- README 与矩阵不再互相矛盾。
- acceptance fixture 在当前 Linux host 可构建；其余 platform 的命令和输入固定。
- 任何发现的 API 虚报先修正或明确标为 degraded，再进入后续阶段。

## 6. 阶段 1：Semantic tree 与 native accessibility correctness {#phase-1}

### 工作 {#phase-1-work}

1. 为 `SemanticTreeBuilder` 和 `AccessibilityUpdate` 增加或完善结构校验：root、
   parent/child、stable node ID、focus target、action registration、bounds、删除、
   reparent 和 snapshot ordering。
2. 针对 cached subtree、deferred/overlay、window close/reopen、inactive window、
   adapter activation/deactivation、late native action 和 repeated frame submission
   写 regression tests。所有 action 必须在正确 foreground window/frame 执行或安全
   拒绝，不能捕获已失效 state。
3. 统一各 desktop adapter 的失败语义、初始化时机、window focus/bounds update 和
   teardown；公共 core 不泄露 OS-specific adapter type。
4. 保持 feature-disabled build 的零语义树开销和明确 unsupported 行为；测试 default、
   `accessibility`、test-support 组合。
5. 将 test-only semantic snapshot API 设计为只读诊断工具，不让生产 component 依赖。

### 退出条件 {#phase-1-exit}

- core snapshot/action/lifecycle suite 覆盖上述 edge case，跨 test context 不串 action。
- macOS、Windows、X11、Wayland adapter 至少 cross-check；当前主机可运行的 native
  adapter 有 runtime evidence。
- Web 的空 bridge 仍明确报告 unsupported，直到有真实 Web accessibility 实现。

## 7. 阶段 2：UI semantic audit 与组件 contract {#phase-2}

### 工作 {#phase-2-work}

1. 建立组件审计表。逐项分类为 interactive、structural、status、text input、
   decorative；仅前四类要求可访问语义。不要给 purely decorative div/icon 添加虚假
   可点击 role。
2. 抽取少量复用 trait/helper：可访问 name/description、disabled/busy/selected/
   expanded/value state、keyboard activation、focus visibility、semantic action 和
   relation。trait 必须与现有 `Focusable`、`Clickable`、`Toggleable` 兼容，而非创建
   平行状态机。
3. 优先迁移 button、toggle、list item、tree item、tab、dropdown/context menu、modal、
   notification、data table、scrollbar、text input/search input。每个交互组件都验证
   pointer 与 keyboard 不分叉，a11y action 调用同一业务 action。
4. 处理 dynamic content：popover/modal 打开后 focus transition、announcement toast、
   disabled item、selected/expanded tree、virtual item visibility 和关闭后的 focus
   restoration。
5. 添加 component-level semantic golden/snapshot tests 及少量 end-to-end action tests；
   golden 只比较稳定、用户可观察字段，避免把 layout noise 固化。

### 退出条件 {#phase-2-exit}

- 高风险 component 清单全部有审计结论、代码位置和测试。
- 没有仅通过 a11y action 才能触发、或仅通过 keyboard 才能触发的业务行为。
- 所有新增用户可见 a11y label 经 i18n 校验。
- UI a11y suite 在 feature 打开时通过；feature 关闭仍能编译且无运行时遗留。

## 8. 阶段 3：Model、selection、focus 与 virtualization 协议 {#phase-3}

### 先行设计实验 {#phase-3-experiment}

在新增 public trait 前，选择一个 tree 和一个 table/list 作为双消费者，写设计说明并
实现最小 prototype。它至少应回答：

- item 的稳定 identity 是什么，删除/移动/reload 后如何防止 ABA 或错误 selection；
- model 的 read、mutation 和 change notification 如何在 Entity/Context ownership
  下发生；
- single/multi/range selection、anchor、keyboard navigation 和 focus ownership 如何
  表达；
- virtual viewport 如何只 materialize 可见项而不让 screen reader 遗失当前项；
- tree parent/child、expanded、loading、drag/drop 与 table row/column 的共同面和
  不同面分别在哪里；
- 何处终止抽象，让 visual styling、editor-specific buffer 和 product command 留在
  消费者层。

### 实施规则 {#phase-3-implementation}

- 先采用 opaque、generation-safe `ItemKey`，不使用可复用裸 `usize` 作为长期
  public identity。
- model 变更以显式 batch/delta 交付；禁止依赖隐式全量 rerender 来掩盖 lifecycle bug。
- selection/focus 只能有一个权威 owner；view 不缓存可变副本。
- virtualized a11y tree 需要有文档化策略：当前 viewport、active descendant 或可访问
  scroll-to-item。不要声称离屏百万节点都同时存在。

### 决策门和退出条件 {#phase-3-gate}

保留抽象的条件是：tree 和 table/list 都减少重复、通过 selection/focus/a11y tests，
在固定大数据集上不比当前实现退化，并且没有将 ZZZ 主题或 editor buffer 泄入协议。
任一条件不成立时，记录 rejection，删除 prototype，保留经修复的专用组件。

## 9. 阶段 4：Text、IME 与 accessible editor boundary {#phase-4}

### 工作 {#phase-4-work}

1. 列出 `PlatformTextSystem`、`TextInputClient`、`InputHandler`、editor input handler
   的 ownership 和 UTF-16/UTF-8 conversion contract；为每项非显然转换添加测试。
2. 使用固定中、日、韩、RTL、emoji、combining mark 和多 cursor corpus 测试 marked
   range、selection、replacement、candidate bounds、focus loss、undo/redo 和 window
   move/resize。
3. 设计 editor 到 semantic text 的窄适配层：屏幕阅读器需要的 name、line/selection、
   caret、range bounds、editable action 和 announcement 从 editor authoritative buffer
   获得，不能复制第二份文本模型到 GPUI。
4. 定义大文件/virtualized text 的可访问策略和性能预算；不一次性导出整个 document。
5. 所有 platform backend 对不支持的 candidate position 或 text action 必须报告正确
   capability，而不是 silent no-op。

### 退出条件 {#phase-4-exit}

- deterministic IME corpus 在 core/editor tests 通过。
- 当前 host 的 Linux IME runtime 重跑；macOS/Windows runbook 可由 QA 直接执行。
- semantic text selection/caret/action 行为在 editor fixture 中可观察且 teardown 安全。

## 10. 阶段 5：Platform capability 与 desktop QA 收敛 {#phase-5}

### 工作 {#phase-5-work}

1. 用阶段 0 fixture 在 macOS、Windows、Wayland、X11 分别运行。每次记录 OS、
   compositor、GPU driver、feature set、locale/IME、assistive technology、命令和
   screenshot/log。软件 renderer 与硬件 renderer 要分开报告。
2. 对每个发现的差异选择一项：修复、degrade with documented capability、或明确
   unsupported。不得只在 UI 层增加 OS string special case。
3. 审计 lifecycle 边界：suspend/resume、GPU device loss、display hotplug、scale
   change、window close/reopen、app quit/reopen、native dialog cancellation、clipboard
   ownership 和 DND cancellation。
4. 让 platform capability tests 覆盖 “advertised capability → 实际 test hook/operation”
   的对应关系。无法运行真实 native service 时保留 integration runbook，不伪造 mock
   通过。

### 退出条件 {#phase-5-exit}

- Tier-1 matrix 通过或明示降级；每个 unsupported 项有用户/调用方可见的 contract。
- VoiceOver、Narrator/NVDA、Orca/AT-SPI 的最小交互脚本有运行结果或外部 blocker。
- README support tier 和 progress ledger 同步更新。

## 11. 阶段 6：API、examples、docs 与升级纪律 {#phase-6}

### 工作 {#phase-6-work}

1. 审计 `gpui` public API，标注稳定、experimental、platform-specific 和 test-only。
   保持 façade 小；优先删除错误/重复 API，而非无限保留 deprecated wrapper。
2. 为 Application/Window/Entity/Render/Element、platform capability、focus/input/IME、
   a11y、lists/virtualization、testing 和 renderer extension boundary 编写任务导向文档。
3. 为每个 Tier-1 支持路径维护可编译 example；examples 不嵌入网络凭据或依赖 hosted
   service。示例行为要覆盖 menu、clipboard、IME/a11y 和 virtual list 的最小用例。
4. 增加 API migration notes。breaking change 要说明替换、删除旧 API 的版本/条件和
   consumer migration test。
5. 让 support matrix、README 和 docs 中同一事实尽可能只来自一个 source。

### 退出条件 {#phase-6-exit}

- 文档描述的 target/feature 与 build matrix 一致。
- 新/修改 example 编译并进入 CI 或有明确的 platform-only runbook。
- public API 没有未说明的 ownership/cancellation/capability 行为。

## 12. 阶段 7：公共 GPU extension RFC 和垂直切片 {#phase-7}

本阶段默认不执行。仅在两个相互独立的真实消费者都无法用现有 Element/Canvas/Surface
完成时开始。

### RFC 必答项 {#phase-7-rfc}

- consumer 的精确 scene/resource 需求以及为何现有 API 不足；
- feature-gated public API、resource ownership、frame scheduling、device loss、
  cancellation 和 teardown；
- WGPU、Metal、headless fallback、WASM 的 capability/degradation 语义；
- shader/source 安全边界、内存预算、profiling 和 error reporting；
- 两个 consumer 与一个 deterministic headless test；
- 移除 prototype 的条件。

### 退出条件 {#phase-7-exit}

只有 vertical slice 在所有目标 backend 有正确输出、无 leak、device loss 可恢复且
不破坏 frame diagnostics，才稳定 API。否则删除 prototype，记录拒绝结论；不得让
内部 `render_api` 因实验而变成半公开接口。

## 13. 阶段 8：收敛、性能与最终验收 {#phase-8}

### 工作 {#phase-8-work}

1. 将所有 phase checklist、support matrix、runtime logs、API docs 和 progress ledger
   对齐。删除迁移期间的 dead code、feature gate、compatibility shim 和重复 tests。
2. 对 core、UI、editor 和 platform crate 运行完整验证；对耗时测试记录 host、GPU、
   feature、数据集、命令和结果。
3. 以旧重构的 fixed workload 为性能回归基线：typing、scroll、resize、cache replay、
   virtual list/table、a11y-enabled frame、IME corpus 和 device loss。correctness failure
   优先于性能；正确后才调整预算。
4. 复核 i18n、license、philosophy、no-default-network 和 native dependency 变化。

### 最终验证矩阵 {#final-validation}

按可用环境运行并记录：

```sh
cargo fmt --all -- --check
cargo test --workspace
./script/clippy
script/check-philosophy
cd docs && npx prettier --check src/
```

此外运行每个已改 crate 的 default、accessibility/test-support 组合、headless renderer
tests、editor IME/a11y tests 与 Tier-1 runtime fixture。无法在当前主机运行的 macOS/
Windows 项必须保留精确命令、环境前提和 `NOT RUN` 状态。

## 14. 风险控制与停止条件 {#risk-control}

| 风险                           | 处理                                                             |
| ------------------------------ | ---------------------------------------------------------------- |
| 把 Qt 面积误当作 GPUI 待办     | 每个新增 API 必须有 ZZZ consumer、第二消费者或 RFC gate。        |
| a11y snapshot 测试掩盖原生错误 | 将 native screen-reader runbook 设为单独证据类别。               |
| model/view 抽象过早            | 双消费者 prototype；失败就删除而不是永久保留。                   |
| capability bool 虚报           | 从 advertised capability 到实际 operation/test 的对应测试。      |
| GPU API 泄露内部生命周期       | 条件 RFC、窄 vertical slice、device-loss/teardown 先行。         |
| 大重构损坏 editor 输入         | 固定 UTF-16/CJK/multi-cursor corpus，每次迁移跑 targeted tests。 |
| 长工期丢失上下文               | progress ledger 记录决策、命令、证据、提交和 next action。       |

发生下列任一情形必须停下实施并更新设计，而非继续堆补丁：无法解释 ownership；测试
需要未定义 sleep/retry 才稳定；两份状态相互同步；一个 capability 在不同后端有相反
语义却被 bool 压平；或只有“像 Qt”而没有消费者能证明 API 的存在理由。
