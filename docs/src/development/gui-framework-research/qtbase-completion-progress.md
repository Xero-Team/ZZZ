---
title: GPUI Qt Base Gap Completion Progress
description: Execution ledger for the GPUI Qt Base gap completion program.
---

# GPUI Qt Base 差距闭环进度账本 {#gpui-qtbase-completion-progress}

本账本记录 [GPUI Qt Base 差距闭环执行计划](./qtbase-completion-plan.md) 的实际工作。
它从既有的 [GPUI 重构进度账本](../gpui-refactor-progress.md) 已完成状态继续，不能将
旧计划的阶段 0–9 重新标记为未完成。

## 执行状态 {#status}

| 项目           | 值                                                               |
| -------------- | ---------------------------------------------------------------- |
| 计划状态       | `ACTIVE`                                                         |
| 当前阶段       | 阶段 0：Baseline、support matrix 与文档事实；阶段 1/2/4/6 已启动 |
| 当前基线       | `434f808bc34b485368fe8927f5f91f774cc2931c` (`main`)              |
| 启动时间       | 2026-10-08                                                       |
| 旧基础设施计划 | 阶段 9 `COMPLETE`；不重新执行                                    |
| 当前工作树     | 本计划新增的 report、plan、goal、progress 和 SUMMARY 文档修改    |

## 继承的事实 {#inherited-facts}

既有执行账本已经验证并完成了 frame diagnostics、threaded dispatcher、AccessKit
core/action routing、Linux AT-SPI/Orca、headless renderer、Window owner 拆分、render
contract、WGPU 模块化、platform capability/lifecycle、Editor IME 与最终收敛。它仍有
如下平台结果，必须作为新 matrix 的输入而非被遗忘：

| 项目                                                  | 继承状态      | 解释                                    |
| ----------------------------------------------------- | ------------- | --------------------------------------- |
| Linux native runtime、XIM、AT-SPI/Orca、RADV/llvmpipe | `PASS`        | 旧账本记录命令和原始日志。              |
| macOS VoiceOver/native IME runtime                    | `NOT RUN`     | 当前 Linux 主机无法执行；已有 runbook。 |
| Windows Narrator/native IME/hardware renderer runtime | `NOT RUN`     | 当前 Linux 主机无法执行；已有 runbook。 |
| Web IME candidate positioning                         | `UNSUPPORTED` | 代码显式报告不支持。                    |
| Web accessibility adapter                             | `UNSUPPORTED` | `WebWindow` 目前仅采用默认空 bridge。   |

## 阶段 0：Baseline、support matrix 与文档事实 {#phase-0}

状态：`ACTIVE`

### 已完成 {#phase-0-completed}

- 已固定当前 HEAD、branch、工作树和计划文档。
- 已复核 `PlatformCapabilities` 默认值、macOS、Windows、X11、Wayland 与 Web 的实现
  位置。当前桌面 backend 在 `accessibility` feature 启用时报告 a11y/IME capability；
  Web 明确报告 `text_input`、candidate positioning 和 accessibility 为 false。
- 已确认 macOS、Windows、X11、Wayland 都有 `AccessibilityBridge` adapter；Web 没有
  native adapter 实现。
- 已修正 `crates/gpui/README.md` 的 macOS/Linux-only 断言。README 现在把 macOS、
  Windows、Linux/FreeBSD 和 WebAssembly 标为 source targets，并明确 Web 的
  text-input、IME candidate positioning 和 accessibility 当前不可用；它不再把 source
  target 误写成未经验证的 runtime support tier。
- 当前主机已安装 macOS、Windows 和 WASM cross targets。macOS accessibility check 与
  WASM `gpui_web` check 通过；WASM 仅有 vendored WGPU 的既有 unused warning，不属于
  ZZZ-owned source warning。
- Windows cross-check 首先揭露了两个真实的 build boundary 问题：`async-tar` 的 Windows
  symlink code 需要 `async-std/unstable`，以及 `gpui_windows` 将 `windows 0.62` 与
  `windows-core`/`windows-numerics`/`windows-registry` 0.100 混用。`http_client` 现在
  仅在 Windows 启用前一 feature；GPUI Windows crate 将后三个 crate 对齐到 Windows
  0.62 的 ABI generation。修复后 locked Windows accessibility cross-check 通过。
- GitHub Actions 现在有 `GPUI platform checks` job，安装 macOS、Windows、WASM targets
  后离线执行三条 cross-check。此 gate 防止 support matrix 退化为仅靠 README 的手工声明。
- 已验证 Linux capability matrix：

  ```sh
  cargo test --locked -p gpui_linux --features accessibility capability_matrix
  ```

  结果：`3 passed; 0 failed`。

- 已执行 cross-target checks：

  ```sh
  cargo check --locked -p gpui_macos --tests --target x86_64-apple-darwin --features accessibility
  cargo check --locked -p gpui_windows --tests --target x86_64-pc-windows-gnu --no-default-features --features accessibility
  RUSTC_BOOTSTRAP=1 cargo check --locked -p gpui_web --tests --target wasm32-unknown-unknown
  ```

  结果：均通过；这只证明 target build。macOS VoiceOver、Windows Narrator/hardware 和
  browser runtime 仍维持各自的 `NOT RUN`/`UNSUPPORTED` 状态。

### Capability inventory v1 {#capability-inventory-v1}

| Target/backend | Advertised source capability                                                                                                          | Automated evidence                                                  | Runtime evidence                                                                       |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| macOS          | AppKit + Metal; text input, candidate positioning, native prompt, accessibility feature, test headless renderer                       | `gpui_macos` capability test cross-check PASS                       | VoiceOver/IME `NOT RUN` on this host                                                   |
| Windows        | Win32 + WGPU; text input, candidate positioning, native prompt, accessibility feature, test headless renderer                         | Windows accessibility test cross-check PASS after ABI/async-tar fix | Narrator, IME and hardware renderer `NOT RUN` on this host                             |
| Linux X11      | X11 + WGPU; text input, candidate positioning, accessibility feature; no platform-window headless renderer                            | `gpui_linux` capability matrix PASS                                 | XIM 100/100 and AT-SPI/Orca PASS in inherited runbook                                  |
| Linux Wayland  | Wayland + WGPU; text input, candidate positioning, accessibility feature; compositor-dependent controls/bell                          | `gpui_linux` capability matrix PASS                                 | No separate fresh Wayland assistive-technology run in this phase                       |
| WebAssembly    | Web canvas + WGPU; frame callbacks and write-only clipboard; no text input, candidate positioning, accessibility or headless renderer | WASM `gpui_web` check PASS                                          | Browser composition/runtime remains `NOT RUN`; accessibility is explicitly unsupported |
| Linux headless | no compositor services, no IME/a11y; test renderer is supplied separately by `gpui_platform`                                          | headless capability matrix PASS                                     | deterministic test only                                                                |

- 已验证当前 UI semantic smoke test：

  ```sh
  cargo test --locked -p ui --features accessibility --test accessibility
  ```

  结果：`1 passed; 0 failed`。

- 已验证 GPUI accessibility-enabled core suite：

  ```sh
  cargo test --locked -p gpui --lib --features accessibility
  ```

  结果：`266 passed; 0 failed`。

- 已写入并格式化差距报告、执行计划和 Goal；`cd docs && npx prettier --check src/`
  通过。

### 待完成 {#phase-0-next}

1. 将 capability inventory 从当前分散 bool 和 backend test 提取为可审查表，逐项连接
   advertised API、实际 backend、automated evidence、native runtime evidence 与支持等级。
2. 审计其余 examples 和 platform docs，决定 support matrix 的单一事实来源及自动/测试
   校验策略。
3. 固定 desktop acceptance fixture 的最小输入和结果格式，复用旧 IME/a11y runbook，
   不重建已通过的 Linux 环境脚本。
4. 对 capability bool 是否需要表达 `degraded` 做设计调查；在有实际歧义前不做
   type-level 重构。

### 下一步 {#next-action}

阅读各 backend 的完整 capability constructors、test hooks 和 README/examples，形成第一版
capability inventory；随后以一个可失败的文档/测试同步 gate 防止 README 再次宣称错误的
平台支持。

## 阶段 1：Semantic tree 与 native accessibility correctness {#phase-1}

状态：`ACTIVE`

### 已完成 {#phase-1-completed}

- 修复重复带 role 的 `ElementId` 会污染语义层级的问题。此前
  `SemanticTreeBuilder::push_node` 拒绝重复 node ID，但没有抑制其后代；后代会挂到此前
  使用同一 ID 的 sibling。builder 现在以 frame-local suppressed scope 表达这个失败，
  并在该 scope 结束后恢复正常 parent。
- 在 `StatefulInteractiveElement` 增加 `aria_description`，让组件能将辅助说明与 label
  分开投影到 AccessKit；frame-level regression 验证完整 snapshot 保留 description。
- 为 builder 添加 unit regression：重复 node 与其 child 不出现，之后的有效 sibling
  仍附着到 root。
- 为真实 Element lifecycle 添加 regression：两个 sibling 重复语义 ID 时，重复 subtree
  不会将后代附着到第一个 sibling，且后续 sibling 仍在完整 snapshot 中。
- 为 node removal 添加 regression：完成帧中的 semantic button 被下一帧移除后，使用旧
  node ID 的 native Click 不再路由到旧 action handler。
- 已验证：

  ```sh
  cargo test --locked -p gpui --lib
  cargo test --locked -p gpui --lib --features accessibility
  cargo test --locked -p ui --features accessibility --test accessibility
  ./script/clippy -p gpui --features accessibility
  cargo fmt --all -- --check
  git diff --check
  ```

  结果：default GPUI `259 passed`；accessibility GPUI `269 passed`；UI semantic
  smoke `1 passed`；clippy 和 philosophy gate 通过。

### 下一步 {#phase-1-next}

审计 semantic tree 的 focus、action、teardown 和 cached/overlay subtree 边界，选择一个
尚未由真实 Element lifecycle 覆盖的可复现错误先加 regression，再考虑任何结构性 API。

## 阶段 2：UI semantic audit 与组件 contract {#phase-2}

状态：`ACTIVE`

### 已完成 {#phase-2-completed}

- `Checkbox` 与 `Switch` 现在将 visual state 映射到 `Role::CheckBox`/`Role::Switch`，
  将 `ToggleState::{Unselected, Indeterminate, Selected}` 映射到
  `Toggled::{False, Mixed, True}`，并在可用时提供 label、disabled state 和 Click action。
- Checkbox 的 click listener 改为 `Rc`，使 mouse 与 a11y action 调用同一个 handler；
  这避免两条路径拥有容易漂移的 state transition。
- `ChoiceCard` 现在是唯一的 semantic control：radio/checkbox variant 分别投影为
  `Role::RadioButton`/`Role::CheckBox`，携带 label、description、selected/toggled state、
  focus 和 Click action。内部 checkbox indicator 明确为装饰，避免 screen reader 重复
  朗读无名称的第二个 checkbox。
- `ListItem` 现在可声明特定 a11y role/toggled state，并复用 pointer Click handler
  注册 AccessKit Click。ContextMenu 使用该能力将根投影为 `Role::Menu`，将普通和
  checked entry 分别投影为 `Role::MenuItem`/`Role::MenuItemCheckBox`。
- `ContextMenu::action_checked(..., false)` 现在保留 unchecked checkbox metadata，避免
  将“可勾选但未选中”的条目退化为普通 menu item；submenu trigger 也使用
  `Role::MenuItem`。
- `Table` 的 `AnyElement` cell 无法可靠推导可访问名称，因此没有把匿名视觉节点错误地
  标为 table cell。调用方可显式提供 `TableAccessibility`：稳定 table ID、table label、
  每列 header label 和按 row/column 解析 cell label 的 callback。
  `Table::with_accessibility` 验证 header 数量必须与列数一致，并在不匹配时返回结构化错误。
- 启用该 opt-in 后，Table、header row、column header、data row 和 cell 分别投影为
  `Role::Table`、`Role::Row`、`Role::ColumnHeader`、`Role::Row` 和 `Role::Cell`。普通
  rows、uniform list 与 variable-height list 共同经过相同的 row/cell 渲染路径；feature
  关闭时不会投影 semantic node。
- 扩展 UI semantic integration test：验证 checkbox/switch 的 role、label、三态和
  AccessKit Click；同时验证 ChoiceCard 的 description、state、Click 和 Focus，以及
  ListItem 的 MenuItemCheckBox override。独立的真实 ContextMenu snapshot test 验证
  Menu、普通 MenuItem、未勾选 MenuItemCheckBox 和 action routing，并确认它们调用与
  鼠标相同的业务 action。
- 扩展 semantic integration test：验证 opt-in table 的完整 role/name 层级，并新增 unit
  regression，拒绝列数与 header 数不一致的配置。
- 已验证：

  ```sh
  cargo test --locked -p ui
  cargo test --locked -p ui --features accessibility --test accessibility
  cargo clippy -p ui --features accessibility --release --all-targets --all-features -- --deny warnings
  script/check-philosophy
  cargo fmt --all -- --check
  git diff --check
  ```

  结果：UI default `71 passed`、doc tests `41 passed`；UI semantic integration tests
  `2 passed`；release clippy 和 philosophy gate 通过。

### 下一步 {#phase-2-next}

继续按 interactive、structural、status、text input、decorative 分类审计组件；优先
DropdownMenu、scrollbar 与剩余 text surface。保持 DataTable 语义为有调用方提供可访问名称
时的 opt-in，不能从 `AnyElement` 猜测 cell 内容。每次只迁移一条完整的
keyboard/pointer/a11y action 路径，不为装饰元素添加 role。

## 阶段 3：Model、selection、focus 与 virtualization 协议 {#phase-3}

状态：`REJECTED FOR CURRENT CONSUMERS`

### 设计实验结论 {#phase-3-decision}

当前代码没有两个可诚实共享同一个模型 owner 的 consumer：

- `TreeViewItem` 是无数据源的单项 visual component。它从 caller 接收 `label`、
  selected、expanded、focus handle 和 callbacks，不拥有 parent/child、loading、move 或
  selection model。
- `Table` 接收 `AnyElement` headers/rows 或只按 `usize` 回调的虚拟行；它不拥有 cell
  值、stable row identity、row selection 或 model mutation。
- `ListState`/`UniformListScrollHandle` 正确地以 index 管理 measurement、scroll 和保持
  offscreen focused item 的渲染，但没有业务 item identity，也不应成为通用数据 model。

因此没有加入 `ItemKey`、`ItemModel`、`SelectionModel` 或公共 model/view trait。这样做
会为一个不存在的 owner 发明 state，并把 table 的 `AnyElement` 表示、tree 的 product
hierarchy和 editor buffer 强行耦合，违反双消费者 gate。

### 重新开启条件 {#phase-3-reopen}

只有同时出现一个拥有 parent/child/loading/expanded 的实际 tree owner 和一个拥有 stable
row identity/selection 的实际 table 或 list owner 时才重开。届时 prototype 必须证明：

1. opaque generation-safe identity 能跨 deletion/move/reload 防止 ABA；
2. model delta、selection anchor、focus 和 scroll-to-item 各只有一个权威 owner；
3. virtualized a11y 有 current viewport 或 active-descendant 策略，而不是虚假导出全部
   离屏项；
4. 两个 consumer 都减少重复且不泄漏 ZZZ theme、editor buffer 或 product command。

在满足上述条件前，保持当前 List/UniformList 的 layout/focus 职责和产品层的数据职责，
是更正确而非更小的设计。

## 阶段 4：Text、IME 与 accessible editor boundary {#phase-4}

状态：`ACTIVE`

### 已完成 {#phase-4-completed}

- `ui_input::InputField` 的 `TextInput` semantic node 现在在非 masked 场景投影当前
  value；`masked(true)` 的字段刻意不导出 value，避免 API key 或 password 被辅助技术
  snapshot 泄露。
- 新增 accessibility regressions：普通 field 验证 label/value/focus；masked field 验证
  secret text 不在 semantic value 中。masked fixture 初始化项目的 i18n service，保留
  mask-toggle tooltip 的正常本地化行为。
- 已验证：

  ```sh
  cargo test --locked -p ui_input
  cargo test --locked -p ui_input --features accessibility
  cargo clippy -p ui_input --features accessibility --release --all-targets --all-features -- --deny warnings
  script/check-philosophy
  cargo fmt --all -- --check
  git diff --check
  ```

  结果：default `1 passed`；accessibility `3 passed`；release clippy 和 philosophy gate
  通过。

### 下一步 {#phase-4-next}

将 InputField semantic value 与底层 editor 的 UTF-16 selection、marked text、caret bounds
和 AccessKit text action 的 authoritative owner 对齐；在此之前不复制第二份 editor text
model 到 GPUI。

## 阶段 6：API、examples、docs 与升级纪律 {#phase-6}

状态：`ACTIVE`

### 已完成 {#phase-6-completed}

- `gpui` package metadata 的 repository 现在指向 ZZZ；移除了没有 ZZZ ownership 的
  GPUI homepage。
- GPUI README 以 in-tree `contexts` doc 和 `hello_world` example 作为学习入口，移除
  upstream `gpui.rs`、Zed Discord 和 Zed blog 链接；issues 指向本仓库。
- `script/check-philosophy` 现在检查 GPUI README/manifest 不再回归到 upstream hosted
  support 或 upstream repository metadata。
- `cargo check --locked -p gpui --examples` 通过，且已加入 `GPUI platform checks` CI job，
  使 README 的 local example 入口持续可编译。

### 下一步 {#phase-6-next}

审计 public GPUI docs/examples 的 ownership、capability、feature-gate、cancellation 和
upgrade semantics；只在 target/build matrix 能验证时增加平台声明。
