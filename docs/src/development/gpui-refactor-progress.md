---
title: GPUI Refactor Progress
description: Execution ledger for the staged GPUI infrastructure refactor.
---

# GPUI 重构进度账本

本账本按
[GPUI 完整重构执行计划](./gui-framework-research/refactoring-plan.md)记录阶段
0–9 的基线、实验、验证、提交和平台欠账。原始日志、benchmark、pixel output
和临时数据保存在 `.tmp/gpui-refactor/`。

## 执行状态

| 项目                       | 值                                         |
| -------------------------- | ------------------------------------------ |
| 工作分支                   | `refactor/gpui-architecture`               |
| 计划基线                   | `152a5eb983a883c69a6cc4eae082312ba75aa9f9` |
| 执行基线                   | `152a5eb983a883c69a6cc4eae082312ba75aa9f9` |
| 基线复核                   | `PASS`：开始执行时 HEAD 与计划基线相同     |
| 通用 Zed reviewed baseline | `decbf641b18f1982b3475c037e7c5c554471574f` |
| 当前阶段                   | 阶段 1：frame diagnostics 与预算           |
| Goal 状态                  | `ACTIVE`                                   |

开始执行时，工作树包含用户已有的 GUI 研究文档修改、未跟踪的计划文档和
`.tmp/ui_ref/` 参考仓库。这些内容原样保留；重构提交只显式暂存本账本和本 Goal
产生的文件。

## 环境基线

| 项目               | 值                                                            | 状态   |
| ------------------ | ------------------------------------------------------------- | ------ |
| Host               | Fedora Linux `7.3.0-0.rc4.260925g165768bb7026.42.fc46.x86_64` | `PASS` |
| Rust               | `rustc 1.98.0 (88d9e12ae 2026-08-18)`                         | `PASS` |
| Cargo              | `cargo 1.98.0 (797e8a9bc 2026-08-05)`                         | `PASS` |
| Target             | `x86_64-unknown-linux-gnu`                                    | `PASS` |
| Session            | Wayland, `DISPLAY=:0`, `WAYLAND_DISPLAY=wayland-0`            | `PASS` |
| GPU                | AMD Radeon 8060S Graphics, RADV Mesa 26.2.3                   | `PASS` |
| Software adapter   | llvmpipe, Mesa 26.2.3, LLVM 23.1.0                            | `PASS` |
| Vulkan             | Instance 1.4.357; RADV and llvmpipe enumerated                | `PASS` |
| Fonts              | Noto Sans Mono, Noto Sans, Noto Serif                         | `PASS` |
| Desktop scale      | GNOME scale default (`0`), text scale `1.0`                   | `PASS` |
| TestPlatform scale | `2.0`                                                         | `PASS` |

原始环境输出：`.tmp/gpui-refactor/phase-0/environment.txt`。

## 固定 workload

所有随机 workload 使用 seed `0x5A5A_4750_5549_2026`。数据集固定到执行基线，
并在阶段 1 的 runner 中补齐自动采样。

| Workload      | 固定输入                                                                     | 阶段 0 状态              |
| ------------- | ---------------------------------------------------------------------------- | ------------------------ |
| Editor typing | 确定性生成的 100,000 行 Rust 文件；输入 1,000 个字符                         | `FIXED`；指标待阶段 1    |
| Editor scroll | 同一文件连续平滑滚动 10 秒                                                   | `FIXED`；runner 待阶段 1 |
| Window resize | 100 次固定尺寸序列                                                           | `FIXED`；指标待阶段 1    |
| Product views | command palette、workspace tabs、settings 的 no-op 与单 entity update        | `FIXED`；runner 待阶段 1 |
| Scene corpus  | text、emoji、SVG、image、clip、shadow、path；1x 与 2x                        | `FIXED`；pixel 待阶段 3  |
| Concurrency   | background-to-main、timer、cancel、panic cleanup、window teardown；100 seeds | `FIXED`；runner 待阶段 2 |

固定输入位于 `.tmp/gpui-refactor/phase-0/workloads/`。100,000 行 Rust fixture
为 5,100,040 bytes，SHA-256 是
`09cd61689dc84551b07e4638df8dd0aa5b5285a1f0946a926c4e8741285a4ae3`；
其余文件的 hash 记录在同目录 `SHA256SUMS`。

## 阶段记录

### 阶段 0：固定 baseline

状态：`COMPLETE`

已完成：

- 复核 HEAD 与计划基线一致。
- 创建并切换到 `refactor/gpui-architecture`，保留已有工作树修改。
- 记录 Rust、host、display、GPU、Vulkan adapter 和字体环境。
- 确认 `.tmp/ui_ref/` 的 9 个参考 checkout 均无工作树修改，并记录 revision。
- 生成固定 seed、100k-line Rust fixture、1,000 字符输入、100 次 resize 序列和
  1x/2x scene corpus manifest。
- 运行 GPUI、平台/WGPU/Linux 和 Editor IME/focus/input 基线验证。
- 记录 isolated clean/incremental GPUI check、no-embed `release-fast` binary 和现有
  Editor Criterion proxy benchmark。

参考仓库 revisions：

| Repository     | Revision                                   |
| -------------- | ------------------------------------------ |
| `adabraka-ui`  | `e158684b23d9cb043fed3989ca252212046dabca` |
| `avalonia`     | `17350180c33b063f0e98abbfd19aa3cae63f5d56` |
| `gpui-ce`      | `c6b17e616a35271183ab49f0da1890ee81953a99` |
| `gpui-kit`     | `edd5d3a42a65bfb51dec22df1623177c7b6db909` |
| `gpui-mobile`  | `c7cab3a43970bd5f1e05d907695404ed73fcdc95` |
| `gpui-rsx`     | `8e0751e9361c08af1ceec702be10b50fbf4e412f` |
| `gpui-toolkit` | `26c5060dd414f567a8e42a4013439cf84eb6afe5` |
| `openswiftui`  | `b17d55b85bb3380a45afeca183426b846e990b98` |
| `zed`          | `a84689073d296dfd39987bc7dd478e43ef76d83a` |

验证账本：

| 命令或检查                                                                          | 结果               | 原始数据/说明                                                                         |
| ----------------------------------------------------------------------------------- | ------------------ | ------------------------------------------------------------------------------------- |
| `git rev-parse HEAD`                                                                | `PASS`             | 与计划基线相同                                                                        |
| reference checkout status                                                           | `PASS`             | 9 个 checkout 均无修改                                                                |
| `cargo check --locked -p gpui`                                                      | `PASS`             | isolated clean build，19.52 s，峰值 RSS 854,912 KiB                                   |
| 同一 `cargo check` incremental                                                      | `PASS`             | 0.42 s，峰值 RSS 190,592 KiB                                                          |
| `cargo test --locked -p gpui`                                                       | `PASS`             | 214 unit + 1 integration，0 failed；46.90 s including build                           |
| `cargo test --locked -p editor ime`                                                 | `PASS`             | 6 passed，0 failed；filter 也命中 4 个名称包含 `time` 的测试                          |
| `cargo test --locked -p editor focus`                                               | `PASS`             | 2 passed，0 failed                                                                    |
| `cargo test --locked -p editor input`                                               | `PASS`             | 8 passed，0 failed                                                                    |
| `cargo test --locked -p editor teardown`                                            | `PASS`             | 0 matched；window teardown coverage 来自 GPUI suite                                   |
| `cargo check --locked -p gpui_platform -p gpui_wgpu -p gpui_linux`                  | `PASS`             | 13.68 s                                                                               |
| `ZZZ_SKIP_EMBED_REMOTE_SERVER=1 cargo build --locked --profile release-fast -p zzz` | `PASS`             | 3m56s；峰值 RSS 20,525,636 KiB                                                        |
| no-embed `target/release-fast/zzz`                                                  | `PASS`             | 5,339,134,088 bytes；ELF with debug info；text+data+bss 271,657,972 bytes             |
| Existing Editor Criterion proxy                                                     | `PASS`             | 1000-cursor input 82.661–85.854 ms；render 100.01–101.90 µs；long-line 0.921–1.297 ms |
| allocation/input latency/pixel/semantic output                                      | `MISSING BASELINE` | 阶段 1 instrumentation、阶段 2 semantics 和阶段 3 renderer 补齐                       |

无效样本：两次未设置 `ZZZ_SKIP_EMBED_REMOTE_SERVER=1` 的 release/bench 构建触发
`remote_server_embed` 六平台交叉构建，已终止并标为 `INVALID SAMPLE`。它们不进入
任何预算比较。

原始日志：`.tmp/gpui-refactor/phase-0/`。

提交：待本阶段账本提交后回填 SHA。

下一步：建立 feature-gated frame journal、统一 `FrameBuildId`、collector/snapshot
API 和 EXP-001/002/011 runner，并测量 disabled/enabled overhead。

### 阶段 1：frame diagnostics 与预算

状态：`IN PROGRESS`

### 阶段 2：并发与 accessibility 边界

状态：`NOT STARTED`

### 阶段 3：真实 headless renderer

状态：`NOT STARTED`

### 阶段 4：拆分 `Window`

状态：`NOT STARTED`

### 阶段 5：render contract 与 WGPU 模块化

状态：`NOT STARTED`

### 阶段 6：platform capability 与 lifecycle

状态：`NOT STARTED`

### 阶段 7：UI 集成边界

状态：`NOT STARTED`

### 阶段 8：invalidation 实验

状态：`NOT STARTED`

### 阶段 9：收敛与最终验证

状态：`NOT STARTED`

## Upstream A/B/C 记录

尚未移植 Zed 代码。开始 frame diagnostics、`ThreadedDispatcher` 或 AccessKit
工作时，先从通用 reviewed baseline 后的候选 commits 逐个分类；core semantics、
action routing 和各 platform adapter 分开记录。

## 外部平台 QA

| 平台/检查                | 当前状态  | 说明                                       |
| ------------------------ | --------- | ------------------------------------------ |
| Linux runtime/headless   | `NOT RUN` | 当前主机可执行，随对应阶段运行             |
| macOS runtime/VoiceOver  | `NOT RUN` | 最终提供 exact runbook                     |
| Windows runtime/Narrator | `NOT RUN` | 最终提供 exact runbook                     |
| Linux Orca               | `NOT RUN` | adapter 完成后在当前主机运行或记录环境阻塞 |
