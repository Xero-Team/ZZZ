---
title: GPUI and Qt Base Gap Analysis
description: Evidence-backed scope comparison and decisions for the next GPUI
  completion program.
---

# GPUI 与 Qt Base 差距调研 {#gpui-qtbase-gap-analysis}

本文把本仓库的 GPUI 与 Qt Base 对照，用于决定下一轮 GPUI 完善工作的范围和
优先级。它不是迁移到 Qt 的提案，也不把 Qt 的全部模块当成 GPUI 必须复制的功能。

> **Decision:** 保持 GPUI 作为 Rust 的低层 GPU UI 框架，保持 ZZZ 的 `ui`、
> `editor` 和其他 crate 的上层职责。优先把桌面平台质量、无障碍语义、可复用
> 组件协议、模型/虚拟化和 API 契约补齐。不要把 Qt Core、QML、网络、数据库或
> 全用途 GPU RHI 无条件并入 GPUI。

本报告对应的实施合同是
[GPUI Qt Base 差距闭环计划](./qtbase-completion-plan.md)。要启动持续实施，使用
[GPUI Qt Base 差距闭环 Goal](./qtbase-completion-goal.md)。

## 1. 结论摘要 {#summary}

GPUI 的编辑器主路径已经成熟：应用状态通过 `Entity` 保存，`Render` 每帧构造
Element 树，Taffy 计算布局，Element 在 `request_layout`、`prepaint`、`paint`
三个阶段构建交互、语义和 `Scene`，平台窗口提交已完成的 scene。这个模型特别适合
代码编辑器的高密度文本、虚拟化、命中测试和完全自绘的界面。

Qt Base 是更大的产品边界。它的构建配置包含 Core、Concurrent、Network、SQL、
XML、DBus、GUI、OpenGL、Widgets、platform support、Test、PrintSupport 和
plugins。Qt 的优势不是一项渲染技巧，而是把大量通用应用基础设施、稳定 ABI 和
多行业平台后端作为同一个发行套件维护。

因此，下一轮工作按以下顺序推进：

1. 为已有能力建立可靠事实：每个桌面后端的 capability、无障碍、IME、clipboard、
   drag-and-drop、DPI、multi-window 和 GPU recovery 都有测试或明确的 `NOT RUN`
   记录。
2. 使已存在的 AccessKit 通路在核心、通用组件和 ZZZ 编辑器之间形成完整的语义、
   focus、selection、action 与事件契约。
3. 从 ZZZ 专用 `ui` 中提取确实可复用的无头模型、选择、焦点和虚拟化协议，而不是
   把 Qt Widgets 的所有控件复制到 `gpui`。
4. 修正 README、示例、支持分级和 API 变更纪律。
5. 只有真实消费者证明需要时，才推进移动/嵌入式后端或公共 GPU 扩展接口。

## 2. 范围、修订与证据限制 {#scope}

| 项目       | 检查对象                                                                                                           |
| ---------- | ------------------------------------------------------------------------------------------------------------------ |
| ZZZ / GPUI | 本地工作树 `434f808bc34b485368fe8927f5f91f774cc2931c`；主 crate 是 `crates/gpui`，平台和 renderer 位于相邻 crate。 |
| Qt Base    | `qt/qtbase` 的 `dev` 分支，`0672d18edc5a90da23f77adf80e1fd2c417b9baf`。                                            |
| 检查时间   | 2026-10-08。                                                                                                       |
| 方法       | 源码、manifest、构建文件、测试和公开接口的静态追踪；没有以相同 workload 运行两者。                                 |

静态源码能证明接口和实现路径，不能证明帧时间、耗电、内存、驱动稳定性或屏幕阅读器
实际体验。本报告把这些标为运行时验证项，绝不以“有 trait”或“能 cross-compile”
代替平台 QA。

先前的 [GUI 基础设施架构研究](./report.md)、
[GPUI 完整重构执行计划](./refactoring-plan.md) 和
[执行进度账本](../gpui-refactor-progress.md) 已完成一轮 Window、renderer、
dispatcher、frame diagnostics、headless renderer、capability 和 IME 基础设施
重构。新计划承接其完成状态，不能把它当作从零开始的待办清单重新执行。

> **Note:** Qt Quick、QML、Qt Quick Controls、Multimedia、WebEngine 和 3D 不在
> Qt Base 这个源码仓库中。它们不能被计入“Qt Base 已有、GPUI 缺失”。

## 3. 架构对齐 {#architecture-alignment}

### GPUI 的垂直路径 {#gpui-vertical-path}

```text
Application + Platform
  → native event loop / window / IME / menu / clipboard / dialogs
  → Entity state + Render
  → Element tree
  → Taffy layout → prepaint → paint
  → BuiltFrame / Scene
  → PlatformWindow::draw
  → Metal on macOS, WGPU elsewhere, Web canvas on WASM
```

`crates/gpui/src/element.rs` 明确说明 Element 树在每帧由 `Render` 构建、布局、
绘制，然后被丢弃；稳定状态在 GPUI Entity 和应用状态中。`Platform` 覆盖窗口、
显示器、文件路径提示、菜单、剪贴板、URL scheme、凭据、键盘和 screen capture
接口。`gpui_platform::current_platform` 已为 macOS、Windows、Linux/FreeBSD 与
WASM 选择后端。

核心导出的 Element 原语是 `div`、`text`、`img`、`svg`、`canvas`、`list`、
`uniform_list`、`surface`、`animation`、`anchored` 和 `deferred`。这是一套
自绘基础，而不是标准 widget 库。ZZZ 的 `ui` crate 以此实现 button、dropdown、
modal、tab、scrollbar、tree item、data table 等产品组件。

### Qt Base 的垂直路径 {#qtbase-vertical-path}

```text
QCoreApplication / QGuiApplication / QObject
  → QPA platform plugin + event dispatcher
  → QWindow / QWidget / dialogs / item views
  → font / input context / clipboard / DnD / accessibility / services
  → paint engine or public QRhi
  → Core + Network + SQL + XML + DBus + Test + PrintSupport + plugins
```

`QPlatformIntegration` 规定平台窗口、event dispatcher、字体、剪贴板、拖放、
输入上下文、无障碍、native interface 和系统服务。`QWidget` 是通用控件基类；
Qt Widgets 还直接提供 button、line edit、combo box、tree/table view、file dialog
和 print dialog。Qt Core 的 `QObject` 元对象、`QPluginLoader` 和
`QAbstractItemModel` 是可发现性、动态插件和 model/view 生态的公共契约。

两者不是同层竞品：GPUI 大致对应 Qt GUI/QPA 加上部分自绘基础，而 Qt Base 还含有
广泛的 Core 和 Widgets 产品面。

## 4. 能力矩阵 {#capability-matrix}

状态含义：**已验证**有直接实现证据；**部分**表示核心可用但跨平台或产品覆盖尚未
验收；**边界外**表示不应由 GPUI 单独承担。

| 维度                                            | GPUI 当前状态                                                                                 | Qt Base 状态                                                                 | 下一轮决策                                                                 |
| ----------------------------------------------- | --------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| 桌面 window、菜单、clipboard、file dialog、键盘 | **已验证**：`Platform` 是统一后端契约。                                                       | **已验证**：QPA 提供相同类别的系统集成。                                     | 不堆叠 API；补 capability 测试和平台矩阵。                                 |
| 自绘渲染与编辑器交互                            | **已验证**：Element 三阶段与 Scene 模型适合 ZZZ。                                             | **已验证**：Widgets/painting/QRhi 多路线。                                   | 保留 GPUI 模型，不迁移为 QWidget 模型。                                    |
| 公共 GPU/RHI                                    | **部分**：`render_api` 是 crate-private；应用面向 Element/Scene。                             | **已验证**：QRhi 公开 resource、graphics 和 compute API。                    | 没有真实消费者时不公开 renderer internals。                                |
| 标准控件                                        | **部分**：核心提供原语；ZZZ 有产品组件。                                                      | **已验证**：Qt Widgets 是通用组件系统。                                      | 抽无头协议和少量可复用组件，不把所有 widget 塞进 `gpui`。                  |
| 通用 model/view                                 | **部分**：已有列表虚拟化，但未见 Qt 式共享 model 协议。                                       | **已验证**：`QAbstractItemModel` 定义数据、变更、拖放与 view 对接。          | 优先设计 stable key、selection、focus、virtualization 约定。               |
| 文本与 IME                                      | **已验证**：font、line layout、glyph raster、UTF-16 selection、marked text 和候选框桥接存在。 | **已验证**：完整 GUI/input context 与 rich-text document API。               | 不制造第二套 editor；补 IME 和可访问文本的运行时证据。                     |
| 无障碍 core                                     | **已验证**：AccessKit semantic tree 与 action routing 已接入 Element/Window。                 | **已验证**：文本、可编辑文本、value、table、action、selection 等专门接口。   | 把重点放到语义覆盖和真实辅助技术结果。                                     |
| 无障碍 desktop adapter                          | **部分且已有实现**：macOS、Windows、X11、Wayland 有 adapter；Web 目前是空 bridge。            | **已验证**：QPA 有 accessibility 接口。                                      | 不把“feature 已编译”当成通过；验证 VoiceOver、Narrator/NVDA、Orca/AT-SPI。 |
| 平台广度                                        | **已验证**：macOS、Windows、Linux/FreeBSD、WASM 源路径。                                      | **已验证（条件构建）**：额外包括 Android、iOS、EGLFS、LinuxFB、QNX、VNC 等。 | 移动/嵌入式仅由产品需求触发。                                              |
| i18n、locale、网络、SQL、XML、DBus、打印        | **边界外**：由 ZZZ sibling crate 和 Rust 生态处理。                                           | **已验证**：Qt Base 作为套件统一提供。                                       | 不复制 Qt Core；明确 crate 级责任和集成规范。                              |
| 动态插件、元对象                                | **部分**：Rust trait、macro 和 ZZZ extension host，不存在 Qt 式动态 C++ widget ABI。          | **已验证**：QObject metadata 与 QPluginLoader。                              | 不为相似性引入不安全 ABI；先定义稳定 extension 用例。                      |
| API 稳定性与文档                                | **部分**：README 明示 pre-1.0 和 breaking changes，平台说明已落后实现。                       | 长期公共 API/发布资产更广。                                                  | 立即建立 support tier、example 编译和迁移纪律。                            |

## 5. 真实差距与优先级 {#priority-gaps}

### 5.1 P0：事实正确、无障碍和平台验收 {#p0-correctness}

GPUI 已在 macOS、Windows、X11 和 Wayland 实现 AccessKit adapter；Linux 的既有执行
账本也记录了 AT-SPI/Orca 验证。剩余问题不是“重新接 AccessKit”，而是保证：

- 每个 capability 只在确实可工作时暴露为可用；
- semantic tree 的 root、稳定 node identity、focus、bounds、action 和生命周期在
  frame cache、window close/reopen、overlay 和 virtualized content 后仍正确；
- 组件级语义不遗漏 name、role、state、value、selection、expanded/disabled 等；
- editor 的多 cursor、selection、composition 与屏幕阅读器文本范围不相互破坏；
- macOS 与 Windows 真实运行结果从 `NOT RUN` 收敛为可复现记录。

当前 `ui` 中直接声明角色的组件集中在 button、list item、modal、toast、tab 和
tree item。它是良好开端，也说明还不能假定所有组件都有同等语义覆盖。

### 5.2 P1：通用组件协议和 model/view {#p1-components-models}

Qt 的 widget/model/view 价值不在像素外观，而在选择、键盘导航、拖放、数据更新、
可访问性和虚拟化都使用一套共享语义。ZZZ 的组件可以做相同的事，但目前很多行为仍
随单个组件实现。

正确的目标是无头的 Rust 协议：稳定 item identity、可观察数据变化、selection、
focus/navigation、row/column/tree relationship、virtual viewport 与 a11y projection。
具体 visual component 保留在 `ui` 或未来经验证的组件 crate。`gpui` 本身仍只拥有
Element、Window、focus、input、layout、scene 与平台原语。

### 5.3 P1：发布与文档契约 {#p1-contracts}

当前 `crates/gpui/README.md` 一方面说 framework pre-1.0，另一方面仍写使用者必须
在 macOS 或 Linux，和现有 Windows/WASM source path 不一致。修复这个问题不只是
文案：要生成或测试 support matrix，记录 feature gate、backend 限制、运行时测试
结果和 `NOT RUN` 项。公共 API 的 breaking 变更必须同时有 migration note、例子和
测试。

### 5.4 P2：公共 GPU 扩展 {#p2-gpu-extension}

Qt QRhi 能让外部代码直接创建 GPU resource 和 compute pipeline；GPUI 有意把 renderer
放在内部。这是安全性和调度控制的取舍，而不是技术落后。只有当一个真实扩展需要
custom pass、shared texture 或 compute，且无法用 `canvas`、surface 或 Element 表达时，
才以 feature-gated、资源生命周期受控的窄接口实验。不得公开当前私有 renderer 的
内部类型作为捷径。

### 5.5 P3：移动、嵌入式和通用服务套件 {#p3-platform-suite}

Android/iOS、framebuffer、QNX、VNC、打印、SQL、XML、DBus 和网络客户端都可能有
价值，但它们不是当前 GPUI 的合理默认范围。每一项都必须先有产品消费者、所有权、
发布方式、测试设备和长期维护人；否则会让核心 UI 框架承担不可验证的表面积。

## 6. 明确不做的事 {#non-goals}

- 不把 Qt Base 的所有模块或 Qt Widgets API 逐类翻译成 Rust。
- 不把 QML/Qt Quick 记为 Qt Base 的功能差距。
- 不以文件数、star、接口数量或静态 cross-compile 代替质量结论。
- 不为“支持插件”暴露 Rust 动态 ABI，或让 extension 绕过 GPUI 的 frame、resource 和
  security boundary。
- 不把 ZZZ 的 i18n、editor buffer、network、database 或 extension-host 职责倒灌进
  `gpui`。
- 不为了表面兼容保留无删除条件的双实现；迁移必须有 feature gate、替换面和移除条件。

## 7. 验收策略 {#acceptance-strategy}

每项实施都需要三个层次的证据：

1. **Core deterministic tests**：semantic snapshot、action routing、focus、selection、
   layout、cache replay、IME range 和 window teardown。
2. **Headless/render tests**：WGPU/Metal 可用时比较 scene 和 pixel 结果，验证 GPU
   resource 生命周期和 device-loss 路径；不可运行的 backend 必须标明原因。
3. **Native runtime runbook**：macOS VoiceOver、Windows Narrator 或 NVDA、Linux
   Orca/AT-SPI 分别记录 OS、backend、feature、步骤、期望、实际、日志和版本。

每个阶段完成前至少运行受影响 crate 的 `cargo check`、targeted test、`./script/clippy`
和格式检查。最终阶段还要运行 workspace test、philosophy check、文档 Prettier，
并更新 progress ledger。完整命令和退出条件以执行计划为准。

## 8. 会改变本结论的证据 {#falsifiers}

以下结果会改变优先级，而不是被解释掉：

- 若 native QA 显示现有 AccessKit adapters 在一个 Tier-1 桌面系统不可用，平台
  adapter correctness 立即升为阻塞项。
- 若共享 model/selection 原型不能同时减少 table/tree/list 的重复代码并保持语义和
  性能，停止抽象化，保留专用组件。
- 若两个独立的真实 extension 都需要相同 GPU primitive，启动窄 GPU extension RFC；
  否则不公开 RHI。
- 若产品明确承诺 mobile 或 embedded target，先做一个端到端垂直切片，再扩平台矩阵。

这些决策保证 GPUI 因实际 ZZZ 需求变得更正确、可维护和可验证，而不是因模仿 Qt
变得更大。
