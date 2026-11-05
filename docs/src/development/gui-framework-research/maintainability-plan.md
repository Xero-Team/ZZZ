---
title: GPUI Maintainability Completion Plan
description: A staged contract for the remaining GPUI maintainability work after the 2026 architecture refactor, grounded in the current ZZZ HEAD and the gpui-kit reference.
---

# GPUI 可维护性闭环执行计划 {#gpui-maintainability-plan}

本文是 [GUI 架构研究报告](./report.md) 与
[GPUI 重构执行计划](./refactoring-plan.md) 的续作。前两份文档解决的是
"是否换框架、如何拆分 runtime 所有权"，其阶段 0–9 已在
[重构进度账本](../gpui-refactor-progress.md) 中标记完成。本文只处理**当前
HEAD 仍存在、且以正确性与可维护性为优先**的剩余项，不重做已完成的决策。

- 计划基线：ZZZ `d1518d139ff6b509d08955579f5f22fd70595a63`
- 执行基线：ZZZ `631a7a0d9d`（2026-11-05；相对计划基线只有 1 个
  doc-only 提交，未触及本计划覆盖的 crate）
- 参考仓库：gpui-kit `929054a81a36a290f65140e20090ce6d561ef526`
- 研究日期：2026-11-05
- 当前状态：**阶段 0–7 全部执行完成**（阶段 6 含 2 项 `DEFERRED`；阶段 7
  有 4 项 pre-existing 结果已确证）。逐阶段证据见
  [progress ledger](./maintainability-progress.md)，详见文末
  [执行状态](#execution-status)。
- 最终修订：2026-11-05（阶段 4/5/6 结果回填，按用户"行为正确优先、允许
  变更公开接口"决策执行阶段 5 全链路 `Result` 化）。
- 上游原则：同血统实现走 `absorbing-upstream` A/B/C 流程；不从二次 fork
  反向复制代码。gpui-kit 为 Apache-2.0，ZZZ 为 AGPL-3.0-or-later，借鉴思想
  与规范，不搬运实现。

## 1. 目标与完成定义 {#goal-and-done}

本计划完成时满足：

1. `crates/gpui` 的公开 API 有一份可比对快照，任何后续改动都能判断
   "是否破坏 consumer"。
2. `window.rs` 的 8198 行被拆成职责单一的 `window/*.rs` 模块，`window.rs`
   只保留类型定义、模块声明与 re-export；**零算法改动、零公开 API 改动、
   零 consumer 编辑**。
3. `PlatformCapabilities`、`GpuSpecs` 等 record-like 配置类型具备演进保护，
   且不需要 workspace 范围内的 struct literal 重写。
4. `crates/gpui/docs/` 具备与内核机制对应的最小可用参考，AI coding agent
   能据此查到真实签名而不是凭训练数据猜测。
5. Web 后端的 "unsupported" 不再只靠 `log::warn!` 表达，调用方与
   `PlatformCapabilities` 的状态一致。
6. 每一阶段都有命令、阈值、证据路径、提交 SHA 与回退方式；无法在本机
   验证的平台项标记 `NOT RUN` 并附 runbook。
7. `cargo test --locked -p gpui --lib`、
   `cargo test --locked -p gpui --lib --features frame-diagnostics,accessibility`、
   `cargo xtask clippy -p gpui`、`cargo xtask check` 与
   `cd docs && npx prettier --check src/` 全部通过，或只剩复现过的
   baseline failure。

不在本计划范围：更换框架、引入第二套 runtime/property system、把
`render_api` 抽成独立 crate（EXP-005 已否决）、scoped invalidation
（EXP-007 已否决）、重写 `crates/ui` 主题系统、恢复已被移除的商业/账号/
遥测表面。

## 2. 现状基线（静态证据） {#baseline}

以下事实来自当前 HEAD 的源码读取，未在本次研究中运行 build/test。

| 观察                     | 证据                                                                     |
| ------------------------ | ------------------------------------------------------------------------ |
| `window.rs` 8198 行      | `crates/gpui/src/window.rs`；230 个 `pub fn`、408 个 `fn`、64 个 impl    |
| 主 `impl Window` 集中    | `window.rs:1033-1551` 与 `window.rs:1567-5696`                           |
| `window/` 只有一个子模块 | `window/prompts.rs`（233 行）；`window.rs:65,71` 声明并 re-export        |
| 内联测试在同文件         | `window.rs:6191` 起 `mod tests`                                          |
| 公开结构体无演进保护     | 307 个 `pub struct`、约 151 个含 `pub` 字段、`#[non_exhaustive]` 为 0    |
| record-like 代表类型     | `platform.rs:530` `PlatformCapabilities`、`gpui.rs:355` `GpuSpecs`、     |
|                          | `colors.rs:9` `Colors`、`style.rs:180` `Style`                           |
| capability 已显式化      | `platform.rs:530-543`；`platform.rs:2631` 有默认全 unsupported 测试      |
| Web 仍 warn-and-return   | `crates/gpui_web/src/platform.rs:136-157,244,258`、`window.rs:719,723`   |
| 内核文档极薄             | `crates/gpui/docs/` 仅 3 文件 233 行；`doctest = false`（Cargo.toml:47） |
| 内核 API 文档门禁        | `gpui.rs:2` `#![warn(missing_docs)]`（warn 而非 deny）                   |
| 生产 `.unwrap()` 仅 1 处 | 位于 `test.rs:217`；121 处 `.expect()` 多为已证不变量断言                |
| 参考项目文档规模         | gpui-kit 有 22 份 GPUI mechanics 参考、405 行 usage、131 行 recipes      |

`keymap.rs` 的 55 处 `.unwrap()` 全在 `keymap.rs:340` 的
`#[cfg(test)] mod tests` 内，属 `.rules:6` 豁免；9 处 `let _ =` 中 4 处是
编译期类型检查示例（`elements/text.rs:1300-1303`），不构成 `.rules:8` 违规。
本计划**不批量改写这些既有调用点**，只在阶段 6 建立"不变量 vs 偷懒 panic"
的可审查标记。

## 3. 排序原则与依赖 {#ordering}

顺序由"正确性与可维护性优先"决定，而不是由改动难度决定：

```text
阶段 0 冻结基线
  ↓
阶段 1 公开 API 快照门禁  ← 后续所有结构改动的安全网
  ↓
阶段 2 window.rs 机械分区（零算法改动）
  ↓
阶段 3 record-like 公开类型演进保护
  ↓
阶段 4 内核机制文档与 agent 参考
  ↓
阶段 5 Web unsupported 语义收口
  ↓
阶段 6 可审查性收尾（不变量标记 / 命名 / 门禁补强）
  ↓
阶段 7 全量验证与移交
```

阶段 1 必须先于阶段 2：8198 行文件拆分最大的风险是"某个 `pub fn` 的
可见性或签名在搬迁中悄悄变化"，没有快照就无法证明零 API 改动。
阶段 3 与阶段 4 可在阶段 2 验证通过后并行，但都依赖阶段 1 的快照。

## 4. 阶段 0：冻结基线 {#phase-0}

### 工作 {#phase-0-work}

1. 记录环境：`git rev-parse HEAD`、`rustc -Vv`、`rustup show active-toolchain`、
   `git status --short`、目标 triple、`.tmp/` 中已有日志的保留状态。
2. 建立 `.tmp/gpui-maintainability/` 作为本计划唯一的原始证据目录。
3. 运行并记录（三次取中位数，CPU pin 沿用 EXP-001 的 runner）：

   ```sh
   cargo check --locked -p gpui
   cargo test --locked -p gpui --lib
   cargo test --locked -p gpui --lib --features frame-diagnostics,accessibility
   cargo xtask clippy -p gpui --features frame-diagnostics,accessibility
   cargo xtask check
   cd docs && npx prettier --check src/
   ```

4. 从 [experiments.tsv](./experiments.tsv) 复用固定 workload 与 seed，
   不新造 benchmark。

### 退出条件 {#phase-0-exit}

- 每条命令有结果、耗时与原始日志路径。
- 既有失败（如 `agent_ui` 6 个 baseline 失败，见进度账本阶段 9）从 clean
  baseline 复现并登记，后续不误判为回归。
- 工作树中用户已有改动保持原样，不 reset、不 clean、不覆盖。

## 5. 阶段 1：公开 API 快照门禁 {#phase-1}

**这是本计划最重要的正确性门禁。**

### 问题 {#phase-1-problem}

`crates/gpui` 是 `publish = true`、被大量 workspace crate 直接依赖的
公开 façade（旧报告基线统计 149 个 crate，该数字未在当前 HEAD 重算）。
`window.rs` 的拆分必须证明"公开表面未变"，但当前没有机器可比对的
公开 API 清单。

### 工作（已实施） {#phase-1-work}

1. 在 `tooling/xtask` 增加 `check gpui-api` 子命令
   （`tasks/check/gpui_api.rs`）。**实现与设计稿不同**：pinned toolchain
   1.99.1 是 stable，`rustdoc --output-format json` 需要 nightly（`-Z`
   被拒绝），`cargo-public-api` 也未安装；CI 不应依赖本机私有 nightly。
   因此改用 `xtask` 已有的 `syn`（+ workspace 已有的 `quote`）解析
   `crates/gpui/src/**/*.rs`，提取公开 struct/enum/trait/fn/type/const/
   static/macro/mod 及 impl 中的公开方法，生成排序后的文本清单。
   属性、注释、函数体与 `cfg` 位置不参与，所以文件搬迁不会被误判为
   API 变化；`#[non_exhaustive]` 这类属性变化也**不**触发 diff，交由
   review 与 `cargo check --workspace` 处理。
2. 快照写入 `crates/gpui/api-snapshot.txt`（2918 条），`--bless` 显式更新。
3. 已加入 `check all`（`gpui_api::check_snapshot` 无参包装）；纯源码解析
   约 0.3 s，不进入 MSRV/stable 快速路径。
4. 负向验证：临时把 `Colors::dark` 改名，门禁精确报出 `+`/`-` 两条差异；
   还原后恢复匹配。

### 阈值（实测） {#phase-1-threshold}

- `cargo xtask check` 全部 10 个 gate 通过，含新门禁。
- 负向测试确认门禁能捕获公开标识符变化。
- 阶段 2 结束后 `check gpui-api` 必须 `PASS` 且 blessed 快照**无 diff**。

### 回退 {#phase-1-rollback}

删除 `check gpui-api` 子命令与快照文件即可，不影响生产代码。

## 6. 阶段 2：`window.rs` 机械分区 {#phase-2}

**状态：`COMPLETE`**

### 决策 {#phase-2-decision}

阶段 4 的重构已把状态下沉到 owner，`Window` 被有意保留为公开 façade
（进度账本 397-537 行）。EXP-004/005 否决了抽独立 crate。本阶段**不推翻
该决策**，只在 `crates/gpui/src/window/` 内按职责切分，`window.rs` 保持
唯一入口与 re-export。

原设计把 8198 行按行段切成 10 个子模块。执行时依据实测调整为：
`window.rs` 前半部分（`DispatchPhase`、`FocusHandle`、`Hitbox`、
`FramePacer`、`InputLatencyTracker`、`Window` struct 定义等）互相耦合，
强拆会引入大量跨模块 `pub(super)`，因此**保留在 `window.rs`**；只搬
出边界清晰的自包含组与 `impl Window` 实现。

用户选择"连 `impl Window` 主块一起拆，接受 `pub(super)`"。量化结果：
`methods.rs` 有 243 个方法、60 个私有/受限方法、41 处 `self.` 私有调用；
按职责切成 3 块后，跨块私有调用需要 10 处 `pub(super)`（比预估的
19–30 少，因为切点选在职责边界）。

### 实际分区（已实施） {#phase-2-layout}

| 文件                      | 行数 | 内容                                                              |
| ------------------------- | ---- | ----------------------------------------------------------------- |
| `window.rs`（保留）       | 3050 | `use`、`mod`/`pub use`、常量、`Window` struct 定义、focus/hitbox/ |
|                           |      | diagnostics 类型、`mod tests`                                     |
| `window/methods.rs`       | 2221 | `impl Window` ×2：构造/状态/focus/async/frame 管线                |
| `window/methods_paint.rs` | 1287 | `impl Window`：绘制 API、布局、hitbox、element state              |
| `window/methods_input.rs` | 1125 | `impl Window`：输入分发、action、prompt、inspector                |
| `window/handles.rs`       | 233  | `WindowId`、`WindowHandle`、`AnyWindowHandle` + impls             |
| `window/element_id.rs`    | 169  | `ElementId` + 全部 `From`/`Display`/`TryInto` impls               |
| `window/content_mask.rs`  | 33   | `DispatchEventResult`、`ContentMask` + impl                       |
| `window/paint_quad.rs`    | 100  | `PaintQuad` + `quad`/`fill`/`outline`                             |
| `window/prompts.rs`       | 233  | 原有，未改动                                                      |

`window.rs` 从 8198 行降至 3050 行（-63%）。每个子模块以
`use super::*;` 访问父模块导入与 `Window` 私有字段；父模块
`mod X; pub use X::*;` 保持 `gpui::Window`、`gpui::ElementId`、
`gpui::quad` 等公开路径不变。`impl Window` 拆成 4 个独立 inherent
impl 块（Rust 允许同类型多个 impl 跨模块）。

`dispatch_action_on_node` 与 10 个跨块 helper（`snap_bounds`、
`snap_stroke`、`snap_border_widths`、`cover_bounds`、
`snapped_content_mask`、`focus_node_id_in_rendered_frame`、
`reset_cursor_style`、`prepaint_inspector`、`paint_inspector`、
`paint_inspector_hitbox`）改为 `pub(super)`。这是对父/兄弟模块的最小
可见性放宽，**不是公开 API 变化**（`pub(super)` 仍限 crate 内）。
`Window` 私有字段无需放宽：Rust 中子模块可访问父模块私有项。

### 门禁修正（阶段 2 触发） {#phase-2-gate-fix}

首次搬 `PaintQuad` 后门禁报 diff，原因是初版快照条目含**源文件路径**。
已修正 `gpui_api.rs`：快照按「item path + kind + signature」排序，
不含路径；`is_public` 收紧为只认 `syn::Visibility::Public`（排除
`pub(crate)`/`pub(super)`/`Inherited`）。修正后快照条目从 2918 降至
2609（309 条 crate 内部项正确排除）。

**往返等价性证明**：把 `PaintQuad` 在 `window.rs` ↔ `window/paint_quad.rs`
之间往返搬迁，以及分区前/分区后各自 bless，两份快照 **byte-identical**；
证明门禁对机械分区免疫，且 `pub(super)` 调整不改变公开 API。

### 约束（执行结果） {#phase-2-invariants}

- 零算法改动：不重排语句、不提取函数、不改 `#[cfg]` 条件、不改
  `use` 语义。仅文件搬迁 + 10 处 `pub(super)` + 1 处 `dispatch_action_on_node`
  `pub(super)`。
- 遵守 `.rules:15`：不创建 `mod.rs`，全部 `window/xxx.rs` +
  `window.rs` 中的 `mod xxx;`。
- 每个分区可独立回退；`methods.rs` 3 块切分依赖 10 处 `pub(super)`，
  回退需一并还原。
- `cargo xtask check gpui-api` 全程零 diff。

### 验证（实测） {#phase-2-validation}

| 命令或检查                                                                | 结果                          |
| ------------------------------------------------------------------------- | ----------------------------- |
| `cargo check --locked -p gpui`                                            | `PASS`                        |
| `cargo check --locked -p gpui --features test-support`                    | `PASS`                        |
| `cargo check --locked -p gpui --features frame-diagnostics,accessibility` | `PASS`                        |
| `cargo check --locked -p gpui --all-targets --features test-support`      | `PASS`                        |
| `cargo xtask check gpui-api`                                              | 零 diff                       |
| `cargo test --locked -p gpui --lib`                                       | 291 passed, 0 failed          |
| `cargo fmt --all -- --check`                                              | `PASS`                        |
| `rustfmt --check window.rs window/*.rs`                                   | `PASS`（`pub(super)` 行重排） |

全 workspace check、跨平台 check 与 clippy 见阶段 7 汇总与后台日志
`.tmp/gpui-maintainability/phase2-*.log`。

### 回退 {#phase-2-rollback}

删除 `window/` 下新增子模块并还原 `window.rs`（备份在工作区历史），
同时 revert 阶段 1 的 `gpui_api.rs` 快照格式修正即可。阶段 1 快照
本身与分区无关，不受影响。

## 7. 阶段 3：record-like 公开类型演进保护 {#phase-3}

### 决策与边界（已调整） {#phase-3-decision}

`crates/gpui` 版本 `0.2.2`，README 明确 "pre-1.0，经常有 breaking
changes"，所以**全仓 307 个 `pub struct` 无 `#[non_exhaustive]` 是刻意
策略，不是缺陷**。依据 gpui-kit coding-guides
（`coding-guides.md:632-639`）只对 record-like 类型收口，且它明确允许
"unrelated existing types can be migrated separately"。

用户决定**扩大到 `Colors`/`Style` 等主题类型并接受更多 consumer 改动**，
因此实际范围比原设计更大。关键硬约束经编译器实测确认：外部 crate
**连 `..Default::default()` 都无法构造 `#[non_exhaustive]` struct**
（`E0639`），所以每个外部构造点都必须改写。

### 工作（已实施） {#phase-3-work}

1. 消费者普查（`rg 'Type\s*{' crates --glob '*.rs'`）：
   `PlatformCapabilities` 23 处、`Style` 17 处、`GpuSpecs` 4 处、
   `Colors` 2 处（全在 `crates/gpui` 内部）。
2. 给 4 个类型加 `#[non_exhaustive]`：`Colors`（`colors.rs:9`）、
   `Style`（`style.rs:180`）、`PlatformCapabilities`（`platform.rs:530`）、
   `GpuSpecs`（`gpui.rs:356`）。实测 `Style` 的 `#[derive(Refineable)]`
   与 `#[non_exhaustive]` 在 crate 内兼容。
3. 外部 crate 构造点统一改为 `T::default()` + 字段赋值（语义等价、零新
   API）：3 个 `Style` 消费点（`image_viewer`、`ui/scrollbar`、
   `workspace/pane_group`）、2 个 `GpuSpecs` 消费点（`gpui_wgpu`、
   `gpui_windows`）、6 个平台 `PlatformCapabilities` 消费点
   （`gpui_linux` headless/wayland/x11、`gpui_macos`、`gpui_web`、
   `gpui_windows`）。`Colors` 无外部构造点，无需改动。
4. `gpui` crate 内部构造点（`Style::default()` 实现、`PlatformCapabilities`
   的 `Default`、test window）保留字面量——Rust 允许同 crate 构造，且
   `Default` 实现本身必须用 `Self { ... }`，改动反而会引入无限递归。
5. `WindowControls` / `WindowServiceCapabilities` / `ClipboardCapabilities` /
   `AppLifecycleCapabilities` 等嵌套 capability 类型**不加**标注：它们不
   是本阶段主题类型，且加了会让 11+8+10+9 处构造点连锁失效，超出合理
   边界。这是有意的收口范围，不是遗漏。
6. 负向验证：在 `image_viewer` 临时恢复 `Style { .. }` 字面量，编译报
   `E0639`，确认收口生效。

### 阈值（实测） {#phase-3-threshold}

- `cargo check --locked --workspace` `PASS`（EXIT=0，无 error）。
- 跨平台：`x86_64-apple-darwin` 与 `x86_64-pc-windows-msvc` 均 `PASS`；
  `gpui_linux`/`gpui_wgpu`/`gpui_web` `--all-targets` 均 `PASS`。
  `x86_64-pc-windows-gnu` target 未安装，未验证（与改动无关）。
- `cargo check -p gpui` 的 default / `test-support` /
  `frame-diagnostics,accessibility` / `--all-targets` 四配置均 `PASS`。
- `cargo test --locked -p gpui --lib`：291 passed，0 failed。
- `cargo xtask clippy`（9 个改动 crate）：`PASS`，EXIT=0。
- `cargo xtask check`：10 个 gate 全通过。
- `cargo fmt --all -- --check`：通过。
- consumer 编辑 11 处（3 个 Style + 2 个 GpuSpecs + 6 个平台），超过原
  ≤10 阈值，但为用户明确授权的"接受更多 consumer 改动"。
- **已知 tradeoff**：`let mut x = T::default(); x.a = ...;` 比 struct
  literal 冗长；这是 `#[non_exhaustive]` 的固有代价，正确性与可演进性
  优先于简洁。未来若提供构造器/builders 可改善，但本阶段不新增只为
  对称而存在的 API。

### 回退 {#phase-3-rollback}

移除标注即恢复原语义；consumer 改动 revert 即可。

## 8. 阶段 4：内核机制文档与 agent 参考 {#phase-4}

**状态：`COMPLETE`**

### 执行结果 {#phase-4-result}

`crates/gpui/docs/` 原有 3 页（`contexts.md`、`key_dispatch.md`、
`accessibility.md`）。按 "Incremental Updates / 不建重复文档"（`docs/AGENTS.md`）
新增 7 页、扩写 1 页，每页都含：一句职责 → 核心类型与所有权表 → 输入到
输出的调用链 → 指向真实源码的链接 → 已知限制 → See also。

| 页面                  | 内容                                    | 主要源文件                                       |
| --------------------- | --------------------------------------- | ------------------------------------------------ |
| `entity.md`（新）     | `Entity<T>` / `WeakEntity<T>` / 观察链  | `app/entity_map.rs`、`app/context.rs`、`app.rs`  |
| `element.md`（新）    | `Element` / `IntoElement` / 三阶段      | `element.rs`、`elements/div.rs`、`frame.rs`      |
| `render.md`（新）     | 帧生命周期 / `Scene` / `BuiltFrame`     | `window/methods.rs`、`scene.rs`、`frame.rs`      |
| `async.md`（新）      | executor / `Task<T>` / `AsyncApp`       | `executor.rs`、`app/async_context.rs`            |
| `focus.md`（新）      | `FocusHandle` / key dispatch / 菜单顺序 | `window.rs`、`key_dispatch.rs`                   |
| `test.md`（新）       | `#[gpui::test]` / `TestAppContext`      | `test.rs`、`app/test_context.rs`、`gpui_macros`  |
| `platform.md`（新）   | trait / capability / `Result` 语义      | `platform.rs`、各平台 crate                      |
| `contexts.md`（扩写） | 保留原有 H2，补类型表 / 所有权流 / 限制 | `gpui.rs`、`app/context.rs`、`app/entity_map.rs` |

与计划的两处差异（均基于实测，避免写错链接）：

- 计划中的 `context.md` 改为**扩写现有 `contexts.md`**，不新建重复页。
- 计划引用的 `window/focus.rs` 在阶段 2 后**不存在**（focus 类型保留在
  `window.rs`）；所有链接改用 `window.rs` 的真实行号。
- `Render` / `RenderOnce` 实际定义在 `element.rs`（非 `view.rs`）；
  `BuiltFrame` 是 `pub(crate)`，文档明确标注为非公开 API。

`crates/gpui/README.md` 的 docs 链接列表已扩到 10 页，"Other Resources"
中平台服务的描述补上 `Result` 语义。`doctest = false`
（`crates/gpui/Cargo.toml:47`）经 `git blame` 查明来自 2022 年仓库级提交
"Disable doctests for all libraries"（`3a1a9ecb45`），不是 gpui 特有的
平台链接约束；保留 `false`，原因记入 `test.md`。

### 验证（实测） {#phase-4-validation}

- 所有 `crates/gpui/src/...:line` 路径用脚本核验存在；抽查 10 处行号均
  指向对应 `pub struct` / `pub trait` / `pub fn` 定义。
- 文档里 64 个大写开头的 API 标识符用 `rg` 反查源码定义，未发现凭空
  编造的符号（"未找到"的 9 个均为变体 / 环境变量 / 泛型截断写法）。
- `cd docs && npx prettier --check src/` `PASS`；
  `npx prettier --check ../crates/gpui/docs/*.md` `PASS`。
- `cargo xtask check`（10 门禁）`PASS`。
- 未新建 skill、未改 `.rules`、未新建 crate；未出现 gpui-kit 组件名或
  Apache-2.0 原文。

### 回退 {#phase-4-rollback}

纯文档改动，删除 7 个新页并还原 `contexts.md` / `README.md` 即可。

## 9. 阶段 5：unsupported 语义收口（公开接口变更） {#phase-5}

**状态：`COMPLETE`**

### 决策 {#phase-5-decision}

原计划把 `-> ()` 的 lifecycle / window 方法用 `capabilities()` 短路 +
`log::debug!` 表达"不支持"，并保留公开签名。这属于"调用方无法区分成功
与明确不支持"的妥协。用户明确要求以行为正确 / 可维护性优先，允许变更
公开接口（"我又没有下游我管他呢"），因此本阶段改为**全链路 `Result` 化**：
让类型系统强制表达"不支持"，消除 warn-only no-op 的假成功。

对齐框架既有惯例：`AccessibilityBridge::update_accessibility`
（`crates/gpui/src/platform.rs` 的 `bail!("... unsupported")`）与
`prompt_for_paths` 早已用 `Result<()>` + `anyhow::bail!` 表达 unsupported。
阶段 5 把该模式推广到生命周期与窗口操作。

### 实际改动 {#phase-5-work}

1. **平台 trait `Result` 化**（`crates/gpui/src/platform.rs`）：
   - `AppLifecycle` 的 `quit` / `restart` / `activate` / `hide` /
     `hide_other_apps` / `unhide_other_apps`：`-> ()` 改为 `-> Result<()>`，
     并给**每个方法一个 `anyhow::bail!("... not supported")` 默认实现**。
     新平台默认"明确不支持"，而非默认"假成功 no-op"。
   - `WindowHost` 的 `minimize` / `zoom`：同样改为 `-> Result<()>` + 默认
     `bail!`。窗口 `activate` 各平台均真支持，保持 `()`。
2. **平台实现收敛**：
   - 真实实现（macOS 全部 6 个、macOS/Windows/Linux Wayland/X11 的
     minimize/zoom、Linux/Windows 的 quit/restart、Test/VisualTest）包
     `Ok(())`；`restart` 内部原本 `log::error!` + `return` 的失败路径改为
     `bail!`，构造期 `Window::new` 的 `zoom()?` 让"请求 Maximized 但后端
     不支持"在构造阶段就报错。
   - warn-only / no-op / `unimplemented!()` 的 unsupported 实现
     （Web 6 个 lifecycle + 2 个 window、Windows/Linux 的 activate/hide
     系列、Linux headless + Test window 的 minimize/zoom）**直接删除覆盖**，
     继承 trait 默认 `bail!`。代码净减少，unsupported 语义统一。
3. **App / Window 高层封装**：`App::quit/restart/activate/hide/
hide_other_apps/unhide_other_apps` 与 `Window::minimize_window/
zoom_window` 全部 `-> Result<()>` 透传平台结果；**删除 `App` 层按
   `lifecycle_capabilities()` 的静默门控与 `Window` 层按
   `window_controls` 的门控**——它们本来就是"假成功"的第二来源。
   `capabilities()` 降级为纯 UI 呈现元数据（菜单/控件是否显示），不再是
   行为真相。
4. **调用点收敛**：约 96 处调用（含 `on_action` 闭包、`cx.update` 闭包、
   examples、benchmarks、宏生成代码）用既有 `.log_err()` 惯例消化
   `Result`；`gpui` 公开 re-export `ResultExt` 供外部 crate 使用；
   `gpui_macros` 生成代码用 `gpui::ResultExt::log_err(...)` 全限定，避免
   依赖用户 crate 的 import。不使用 `let _ =`。
5. **回归测试**（`crates/gpui/src/platform.rs` `mod tests`）：
   `default_lifecycle_operations_report_unsupported_instead_of_succeeding`
   断言未覆盖 trait 默认实现的 6 个方法全部返回 `Err`；
   `default_lifecycle_capabilities_are_all_unsupported` 锁定默认
   capability 与之对齐。
6. **门禁 bless**：`crates/gpui/api-snapshot.txt` 13 处签名 `() ->
Result<()>`（+ 参数名因默认实现加 `_` 前缀的变化），逐条复核无意外
   条目。门禁 `collect_items` 不捕获 `Item::Use`，故新增的
   `pub use gpui_util::ResultExt` 不在快照内——这是阶段 1 静态快照的
   已知边界，非遗漏。

### 行为变化（桌面与 Web） {#phase-5-behavior}

- **成功路径不变**：所有真实现返回 `Ok(())`，与原来的 `()` 语义等价。
- **unsupported 路径可观察**：调用 `quit()` / `minimize()` 等时，Web /
  不支持操作的桌面后端返回 `Err`，调用方 `.log_err()` 记录，而非
  静默成功 + 一行 warn。
- **失败信号在 `Result` 类型里**：需要判定的调用方可以直接 `?` 或
  `if let Err(...)`，无需解析日志文本或二次查询 capability。
- **注意**：`activate` 在 Linux / Windows 的启动与打开文件路径无
  `#[cfg]` 守卫且 capability 为 false，现在会如实记一条 error 日志
  （"activating the application is not supported"）。这是删除静默门控后
  的预期结果；保留 `.log_err()` 一致性，不引入 warn/debug 的第二套规则。

### 验证（实测） {#phase-5-validation}

| 命令或检查                                                                | 结果                   |
| ------------------------------------------------------------------------- | ---------------------- |
| `cargo check --locked --workspace`                                        | `PASS`（无 warning）   |
| `cargo check --locked -p gpui --all-targets --features test-support`      | `PASS`                 |
| `cargo check --locked -p gpui --features frame-diagnostics,accessibility` | `PASS`                 |
| `cargo test --locked -p gpui --lib`                                       | 293 passed, 0 failed   |
| `cargo check --locked -p gpui_macos --target x86_64-apple-darwin`         | `PASS`                 |
| `cargo check --locked -p gpui_windows --target x86_64-pc-windows-msvc`    | `PASS`                 |
| `RUSTC_BOOTSTRAP=1 cargo check --locked -p gpui_web --target wasm32-...`  | `PASS`                 |
| `cargo xtask check gpui-api`                                              | 零 diff（13 处 bless） |
| `cargo fmt --all -- --check`                                              | `PASS`                 |
| `cargo xtask clippy`（全量）                                              | 见阶段 7 汇总          |

以上是阶段 5 完成当时的实测结果。收尾复核时用户工作树仍在变化，
`cargo check --workspace` / 全量 clippy / 全量 fmt 的当前状态见阶段 7
与 [progress ledger](./maintainability-progress.md) 的 pre-existing 说明。

Web target 需 `RUSTC_BOOTSTRAP=1`（`wasm_thread` 依赖的
`#![feature(stdarch_wasm_atomic_wait)]` 仅 nightly 可用），与阶段 5 改动
无关，基线即如此。`cargo test -p gpui_web` 在 host 无法运行：该 crate
的全部依赖都在 `cfg(target_family = "wasm")` 下。

### 回退 {#phase-5-rollback}

单主题变更：revert trait 签名与默认实现、各平台实现、App/Window 封装、
约 96 处调用点收敛、`ResultExt` re-export、2 个测试，并重新 bless
`api-snapshot.txt`。`capabilities()` 的门控回退需一并还原，否则会重新
引入"假成功"第二来源。

## 10. 阶段 6：可审查性收尾 {#phase-6}

**状态：`COMPLETE`（含 2 项 `DEFERRED`）**

低风险观察项，逐项核验，**未演变成批量改写**：

1. **不变量标记 — `DEFERRED`**：复核 `elements/text.rs`、`view.rs`、
   `key_dispatch.rs` 的 `.expect()`，其 message 本身已是精确的不变量
   声明（例如 `"measurement has not been performed"`、
   `"downcast should succeed"`），补 `debug_assert!` 或 "why" 注释是重复，
   不增加 review 可判定性。按"任一项成本超预期即 DEFERRED"且"不得批量
   改写"保留原状，不改逻辑、不换 `?`。
2. **命名一致性 — `COMPLETE`**：阶段 2 后 `ElementId` 的 17 个 `From`
   impls 全部集中在 `crates/gpui/src/window/element_id.rs:69-165`，
   其它文件无 `for ElementId` 的 `From` 实现。无需重命名任何公开标识符。
3. **`set_menus` / `bind_keys` 顺序 — 部分 `COMPLETE`**：实测属实。
   `App::set_menus`（`app.rs:2410`）在调用瞬间把
   `&self.keymap.borrow()` 传给平台；macOS 后端
   （`gpui_macos/src/platform.rs:1076`）用该 keymap 在
   `create_menu_bar` 里烘焙菜单快捷键。约束已写入
   `crates/gpui/docs/focus.md` 的 "Menu bar and key-binding order"。
   **回归测试 `DEFERRED`**：烘焙发生在 macOS 原生菜单代码，host 侧单测
   无法覆盖；让 `TestPlatform` 记录 `set_menus` 收到的 keymap 需新增测试
   设施，超出"低风险观察项"范围。
4. **门禁补强评估 — 保留 `warn`**：`cargo check -p gpui --all-features`
   报 0 个 missing documentation 警告；`cargo xtask clippy` 已用
   `--deny warnings` 把 `warn` 事实升级为 error 且通过。提升到
   `#![deny(missing_docs)]` 技术上可行（0 违规）但无实质收益，且会让
   裸 `cargo check` 与仓库其它 crate 的 `#![warn]` 惯例不一致。保留
   `warn`，登记理由，不借机补文档。gpui 内有 47 处
   `#[expect(missing_docs)]` 合法豁免。

阶段 6 未改动任何生产代码；仅 `focus.md` 新增一段文档约束。

## 11. 阶段 7：全量验证与移交 {#phase-7}

**状态：`COMPLETE`**（4 项 pre-existing 结果已确证）
详见 [progress ledger](./maintainability-progress.md)。

### 验证矩阵（实测） {#phase-7-matrix}

| 命令                                                                           | 结果                                           |
| ------------------------------------------------------------------------------ | ---------------------------------------------- |
| `cargo xtask check`                                                            | `PASS`（10 门禁，含 `gpui-api` 零 diff）       |
| `cargo xtask check gpui-api`                                                   | `PASS`（2609 条，13 处 bless 已复核）          |
| `cargo test --locked -p gpui --lib`                                            | `PASS`（293 passed, 0 failed）                 |
| `cargo test --locked -p gpui --lib --features frame-diagnostics,accessibility` | `FAIL`（5）— **基线既有**，见下                |
| `cargo test --locked -p gpui_platform -p gpui_wgpu -p gpui_linux`              | `PASS`（0 tests）                              |
| `cargo check --locked --workspace`                                             | `PASS`（验证期）/ 收尾时用户改动 `E0597`       |
| `cargo check --locked -p gpui --all-targets --features test-support`           | `PASS`                                         |
| `cargo check --locked -p gpui_macos --target x86_64-apple-darwin`              | `PASS`                                         |
| `cargo check --locked -p gpui_windows --target x86_64-pc-windows-msvc`         | `PASS`                                         |
| `RUSTC_BOOTSTRAP=1 cargo check --locked -p gpui_web --target wasm32-...`       | `PASS`                                         |
| `cargo xtask clippy -p gpui -p gpui_macros -p zzz -p xtask`                    | `PASS`（`--deny warnings`，4 pass）            |
| `cargo xtask clippy`（全量）                                                   | `BLOCKED`（pre-existing），见下                |
| `cargo fmt --all -- --check` / `git diff --check`                              | `BLOCKED`（fmt，pre-existing）/ `PASS`（diff） |
| `cd docs && npx prettier --check src/`                                         | `PASS`                                         |
| `cargo test --locked --workspace`                                              | `NOT RUN`（可选，耗时长）                      |

**四项 pre-existing 结果**：用干净 worktree（`HEAD 631a7a0d9d`）跑测试
对照、用 `git diff` 对比用户改动区域后确证，均与本计划改动无关——

- `frame-diagnostics,accessibility` 下 5 个失败基线同样出现（基线
  `306 passed; 5 failed`，本工作树 `308 passed; 5 failed`，多出的 2 个是
  阶段 5 新增且通过的测试）。涉及文件 `deferred.rs` / `window.rs` tests
  模块未被改动。
- 全量 `cargo xtask clippy` 因用户已有工作树改动
  `crates/acp_thread/src/acp_thread.rs:26` 新增 `ProjectPath` 导入触发
  `unused import` → `--deny warnings` 升级为 error 而失败。基线 HEAD 无该
  导入；本计划未改动 `acp_thread`，按纪律不触碰。改动范围的限定版
  clippy 已通过。
- 全量 `cargo fmt --all -- --check` 的 4 个文件 diff 位于
  `acp_thread.rs` / `markdown_preview_view.rs` / `project_tests.rs` /
  `worktree_tests.rs` 的用户改动区域；本计划实际编辑的 `.rs` 文件逐一
  `rustfmt --check` 全部干净。

沿用 [experiments.tsv](./experiments.tsv) 的 EXP-001/002/011 阈值作为
性能回归门槛；本计划不新增 benchmark workload，三次 paired clean check
未重跑（分区已由门禁零 diff 与编译通过证明等价，无增量性能基准采集）。

### 平台欠账 {#phase-7-platform}

以下项在本 Linux 主机标 `NOT RUN`，沿用进度账本已有 runbook，不重复
造轮子：

- macOS VoiceOver / native accessibility runtime（EXP-006）
- Windows Narrator / WGPU headless hardware runtime（EXP-003/006）
- macOS/Windows native IME runtime（EXP-009）
- Web IME candidate positioning（已显式 unsupported）

### 移交（已完成） {#phase-7-handoff}

1. 已新建 [maintainability-progress.md](./maintainability-progress.md)
   账本，逐阶段记录 baseline、命令、结果、证据路径、`PASS`/`FAIL`/
   `NOT RUN` 与 next action。阶段 0–7 的改动已拆为 6 个 `git commit -s`
   提交落在 `main`，提交 SHA 清单与落地复核见账本[提交记录](./maintainability-progress.md#commits)。
2. 已在计划顶部补最终状态与 revision（阶段 0–6 完成、阶段 7 完成，
   阶段 6 含 2 项 `DEFERRED`）。
3. 已在 [SUMMARY.md](../../SUMMARY.md) 登记 progress ledger 链接。
4. `Suggested .rules additions` 两条已写入 progress ledger，未直接改
   `.rules`；实际 PR 描述需再引用。

## 12. 提交与工作区纪律 {#discipline}

- 执行期纪律：在隔离分支 `refactor/gpui-maintainability` 或 worktree 中
  工作；不 push、不开 PR、不合并到 `main`，除非用户明确要求。阶段 0–7
  完成后已按用户要求拆为 6 个 `git commit -s` 提交落在 `main`，见
  [progress ledger](./maintainability-progress.md#commits)。
- 每个提交只含一个可独立回退的行为或结构变化，`git commit -s`。
- 落地时按主题合并为 6 个提交：`Result` 化、`#[non_exhaustive]` 与
  `window.rs` 分区共享 `platform.rs` 及各后端 `window.rs`，物理上交错，合并
  为一个提交以保证每个提交可独立编译；其余各主题各自一个提交。清单见
  [progress ledger](./maintainability-progress.md#commits)。
- 原始日志、API 快照 diff、benchmark 输出放
  `.tmp/gpui-maintainability/`，只把摘要与可复现命令写入 tracked 文档。
- 参考仓库保持只读；不增加 `upstream` remote，不用 `script/cherry-pick`。
- 工作树中已有的用户改动保持原样，不 stage、不 revert、不覆盖。
- 默认构建不启用 profiler / benchmark / diagnostics 开销
  （沿用 refactoring-plan.md:124 的契约）。

## 13. 风险控制与停止条件 {#risk-control}

| 风险                              | 处理                                                           |
| --------------------------------- | -------------------------------------------------------------- |
| 拆分 `window.rs` 悄悄改了公开 API | 阶段 1 快照逐提交比对；有 diff 立即停止                        |
| 子模块间需要放宽可见性            | 记录理由；优先 `pub(super)`，禁止新增 `pub` 字段               |
| 机械搬运引入 `#[cfg]` 平台差异    | 每分区后跑 default + frame-diagnostics + accessibility 三配置  |
| `#[non_exhaustive]` 引发全仓改动  | 先普查；>10 处就缩小到单类型或 `DEFERRED`                      |
| 文档写出不存在的 API              | 每个标识符 `rg` 到当前源码定义                                 |
| Web 改动影响桌面后端              | 只改 `gpui_web`；桌面 trait 宽度不变                           |
| 借机批量改写既有 `.expect()`      | 阶段 6 只加标记，不改逻辑                                      |
| 把 gpui-kit 组件规范误用于内核    | 只采用分层/命名/文档/演进保护四类；组件 catalog 不进本计划     |
| 长工期丢失上下文                  | progress ledger 记录每阶段决策、命令、证据、提交与 next action |

发生下列任一情形必须停下实施并更新设计，而不是继续堆补丁：

- `check gpui-api` 在阶段 2 出现无法用 `--bless` 解释的差异；
- 拆分后 clean check 中位耗时超过 baseline×1.10 且无法归因；
- 任一阶段需要修改超过 10 个 consumer 调用点；
- 出现"两份状态相互同步"或"一个 capability 在不同后端语义相反却被
  bool 压平"；
- 测试需要未定义 sleep/retry 才稳定。

## 14. 与既有决策的关系 {#relation}

| 既有决策                                  | 本计划态度                                     |
| ----------------------------------------- | ---------------------------------------------- |
| 保留 `gpui` 为公开 façade（报告结论 6）   | 继承；阶段 2 只在 façade 内部改模块布局        |
| `render_api` 保留为内部模块（EXP-005）    | 继承；不抽 crate                               |
| scoped invalidation `REJECTED`（EXP-007） | 不重开                                         |
| 平台 capability 显式化（阶段 6）          | 继承；阶段 5 只补调用路径一致性                |
| `window.rs` 状态已下沉（阶段 4）          | 继承；本计划处理的是剩余文件体积，不是重做拆分 |
| macOS/Windows runtime `NOT RUN`           | 继承；附既有 runbook，不冒充已验证             |
| pre-1.0 允许 breaking changes（README）   | 尊重；阶段 3 只对 record-like 类型渐进收口     |
| `cargo xtask check` / locale / philosophy | 不弱化；阶段 7 全量跑                          |

## 15. 最小证伪实验 {#falsification}

| 建议                     | 能推翻它的实验                                                                   |
| ------------------------ | -------------------------------------------------------------------------------- |
| `window.rs` 分区可零风险 | 分区后 `check gpui-api` 出现非 bless 可解释 diff，或 clean check > baseline×1.10 |
| consumer 改动可接受      | 阶段 3 实测 11 处外部构造点全部改写，全 workspace 与跨平台 check 通过            |
| 文档差距值得补           | 补完后 agent 仍以相同频率猜错 API，说明瓶颈在 rustdoc/signature 而非文档页       |
| Web 语义收口不影响桌面   | `cargo test --locked -p gpui_platform -p gpui_linux` 出现新失败                  |

把失败或 inconclusive 结果保留在 progress ledger，不用更干净的 rerun
替换。

## 16. 执行状态 {#execution-status}

| 阶段 | 状态       | 结果                                                                |
| ---- | ---------- | ------------------------------------------------------------------- |
| 0    | `COMPLETE` | 基线 `631a7a0d9d`；`cargo check -p gpui` 8.21 s `PASS`              |
| 1    | `COMPLETE` | `check gpui-api` 门禁落地（syn 快照 2609 条）；加入 `check all`     |
| 2    | `COMPLETE` | `window.rs` 8198→3052 行，8 子模块，11 处 `pub(super)`，门禁零 diff |
| 3    | `COMPLETE` | 4 个类型加 `#[non_exhaustive]`，11 处外部构造点改写，全量验证通过   |
| 4    | `COMPLETE` | 7 新页 + 扩写 `contexts.md`，全部符号 `rg` 核验，prettier 通过      |
| 5    | `COMPLETE` | 全链路 `Result` 化，13 处公开签名变更，293 tests 通过               |
| 6    | `COMPLETE` | 2 项完成、2 项 `DEFERRED`，未改生产代码                             |
| 7    | `COMPLETE` | 全量验证 + 账本/SUMMARY 移交；4 项 pre-existing 已确证              |

原始日志位于 `.tmp/gpui-maintainability/`（不 tracked）：
`phase0-cargo-check.log`、`phase3-workspace-after.log`、
`phase3-gpui-tests.log`、`phase3-clippy-final.log` 等。

工作区中同时存在用户已有的 `crates/telegram_ui/src/telegram_panel.rs`
改动（去掉 `Box::leak`）。本计划执行期未触碰、未 stage、未回退该文件；
落地时它随阶段 5 的 `Result` 化只改了 `cx.activate(true).log_err()` 这一
调用点（见 `dcd5c864a7`），用户原有的 `Box::leak` 改动保持不变。

阶段 2 是后续工作的硬前提：8198 行 `window.rs` 的分区必须在
`check gpui-api` 快照保护下逐提交进行，每分区后立即比对，出现非 bless
可解释的 diff 即停止。
