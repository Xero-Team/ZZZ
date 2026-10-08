---
title: GPUI Qt Base Gap Completion Progress
description: Execution ledger for the GPUI Qt Base gap completion program.
---

# GPUI Qt Base 差距闭环进度账本 {#gpui-qtbase-completion-progress}

本账本记录 [GPUI Qt Base 差距闭环执行计划](./qtbase-completion-plan.md) 的实际工作。
它从既有的 [GPUI 重构进度账本](../gpui-refactor-progress.md) 已完成状态继续，不能将
旧计划的阶段 0–9 重新标记为未完成。

## 执行状态 {#status}

| 项目           | 值                                                            |
| -------------- | ------------------------------------------------------------- |
| 计划状态       | `ACTIVE`                                                      |
| 当前阶段       | 阶段 0：Baseline、support matrix 与文档事实                   |
| 当前基线       | `434f808bc34b485368fe8927f5f91f774cc2931c` (`main`)           |
| 启动时间       | 2026-10-08                                                    |
| 旧基础设施计划 | 阶段 9 `COMPLETE`；不重新执行                                 |
| 当前工作树     | 本计划新增的 report、plan、goal、progress 和 SUMMARY 文档修改 |

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
- 已验证 Linux capability matrix：

  ```sh
  cargo test --locked -p gpui_linux --features accessibility capability_matrix
  ```

  结果：`3 passed; 0 failed`。

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
