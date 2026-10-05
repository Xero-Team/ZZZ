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
| 2026-10-04 续作基线        | `101f0985632b6134ee096308ed02cc7bdaea5175` |
| 2026-10-05 续作基线        | `c4fc7df24af6e096efb01fece5a6dafec0145f2e` |
| 基线复核                   | `PASS`：开始执行时 HEAD 与计划基线相同     |
| 通用 Zed reviewed baseline | `decbf641b18f1982b3475c037e7c5c554471574f` |
| 当前阶段                   | 阶段 4/6/7 收敛                            |
| Goal 状态                  | `ACTIVE`                                   |

开始执行时，工作树包含用户已有的 GUI 研究文档修改、未跟踪的计划文档和
`.tmp/ui_ref/` 参考仓库。这些内容原样保留；重构提交只显式暂存本账本和本 Goal
产生的文件。

### 2026-10-04 续作基线复核

续作开始时工作树 clean，但当前 checkout 位于 `main`。在修改代码前从
`101f0985632b6134ee096308ed02cc7bdaea5175` 创建并切换到
`refactor/gpui-architecture`，没有 reset、clean、覆盖或丢弃现有内容。

相对最后一笔进度账本提交 `ee456111775ac48dc0e90c0903a7f873dc70a79a`，当前
HEAD 已通过 `34e859630f` 合入另一条本地 `main` 历史，并在本计划覆盖范围修改 63
个文件。逐文件 diff 复核结果：变更主要是 fallible operation 错误可见性、dead-code
清理、dispatcher recent timing 查询和 UI/workspace 清理；没有恢复账号、遥测、协作、
原生 Agent 或其它商业表面，也没有撤销 frame/interaction/text-input/render/platform
owner 边界。两项直接兼容修复分别恢复
`PlatformInputHandler::character_index_for_point`（`9598cf0b72`）并补齐 macOS
`WindowHost` trait import（`405a750eff`）。

续作基线验证：

| 命令或检查                                                                                                           | 结果            | 证据                                                                                      |
| -------------------------------------------------------------------------------------------------------------------- | --------------- | ----------------------------------------------------------------------------------------- |
| covered-crate diff `ee45611177..101f098563`                                                                          | `PASS`          | 63 files；318 insertions、659 deletions；未发现 owner/capability/philosophy 回退          |
| `cargo check --locked -p gpui -p gpui_platform -p gpui_wgpu -p gpui_linux -p gpui_macos -p gpui_windows -p gpui_web` | `PASS`          | 13.39 s；`.tmp/gpui-refactor/continuation-2026-10-04/covered-crates-check.log`            |
| GPUI lib test，`frame-diagnostics,accessibility`                                                                     | `FAIL` → `PASS` | 首轮发现跨 test context 的 WindowId journal 串扰；修复提交 `7869cc90ff`                   |
| 同一 230-test GPUI suite 连续运行 10 次                                                                              | `PASS`          | 2,300 tests，0 failed；`.tmp/gpui-refactor/continuation-2026-10-04/gpui-lib-test-10x.log` |
| `cargo fmt --all -- --check` / `git diff --check`                                                                    | `PASS`          | continuation fix 已格式化，无 whitespace error                                            |

续作下一步：继续收敛阶段 1/2/3/5/6/7 的开放验证项；在 EXP-001/002 正式 workload
完成前不进入 scoped invalidation 产品实现。

### 2026-10-05 续作基线复核

续作开始时工作树 clean，但 checkout 再次位于 `main@c7a5c71f47`。现有专用分支
`refactor/gpui-architecture` 停在 `c4fc7df24a`，且 `main` 已在 `139fb6be52`
合入该分支后继续加入协作设置和构建升级。为避免把与本 Goal 无关的协作表面带回
重构历史，在修改代码前切回现有专用分支；没有 reset、clean、覆盖、rebase 或合并
`main`。

基线复跑发现全 feature 的 235-test GPUI 并行套件仍可让不同 `TestAppContext` 复用
相同 `WindowId`，使全局 frame journal 的 skipped-submission 事件串入另一个测试。
`dfcff1a2cd` 为每个 window invalidator 增加 feature-gated journal source ID，并让
`FrameTimingCollector::for_window` 按 window instance 隔离；默认构建不增加字段或开销。
修复后同一套件连续运行 20 次，4,700 tests 全部通过。原始日志：
`.tmp/gpui-refactor/continuation-2026-10-05/gpui-lib-test-20x-source-filter.log`。

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

提交：`6c44518908f6fb461f7cec7598fac87a623e6db8`。

下一步：建立 feature-gated frame journal、统一 `FrameBuildId`、collector/snapshot
API 和 EXP-001/002/011 runner，并测量 disabled/enabled overhead。

### 阶段 1：frame diagnostics 与预算

状态：`COMPLETE`

本阶段已完成的结构改动：

- 在 `gpui` 增加关闭状态零运行时存储的 `frame-diagnostics` feature；默认 feature
  不增加 frame event ring、计时字段或原子计数。
- 增加 `FrameBuildId`、dirty reason、input provenance、render phase、draw 和
  present 事件类型，以及有界 ring-backed `FrameTimingCollector` snapshot API。
- 将 initial/entity/window invalidation、keyboard/pointer dispatch、request-layout、
  prepaint、paint、cache replay、draw completion 和 present 关联到同一 build ID。
- 保留 `request_layout → prepaint → paint`、cached replay、input dispatch 和
  Entity/Context 所有权语义；新增端到端 TestPlatform 检查验证 coalescing 和 input
  provenance。
- 增加 deterministic `frame_diagnostics_runner`：固定 100 次 dirty frame 和 100 次
  keyboard input，输出 draw/phase/input-to-present percentile 与 ring 丢失计数。
- 使用每个 Window 的本地 SmallVec 批量收集事件，在 present 时一次性 flush 到全局
  ring，避免每个 phase/invalidation 都获取全局锁。
- renderer contract 传播 backend 的真实 submission outcome；未提交帧记录
  `FrameEvent::SubmissionSkipped`，不会再错误计入 presented/input-latency 样本。
- 增加 production-like `editor_frame_workload` runner，使用同一 100,000 行 Rust
  fixture 执行 1,000 次输入、600 帧（10 s × 60 Hz）平滑滚动、100 次 resize 和
  1,000 个 cached frame，并同时报告逐帧 percentile、allocation 和 peak RSS。
- printable-key 的 text-input dispatch 现在与原始 keyboard event 使用同一 provenance；
  新 regression test 和真实 Editor runner 都验证 1,000/1,000 presented frames 带有
  input-to-present 样本。
- per-window collector 现在使用全局唯一的 feature-gated source ID，而不是只按可跨
  test context 重复的 `WindowId` 过滤。
- command palette、workspace tabs 和真实 Settings UI 都有固定 workload，输出
  per-frame p95、phase operation 和 cache replay counts。

阶段 1 验证：

| 命令或检查                                                                                                         | 结果                        | 原始数据/说明                                                                                                                                                                                                                                                                   |
| ------------------------------------------------------------------------------------------------------------------ | --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cargo check --locked -p gpui`                                                                                     | `PASS`                      | 0.40 s，RSS 189,612 KiB；`.tmp/gpui-refactor/phase-1/check-default.log`                                                                                                                                                                                                         |
| `cargo check --locked -p gpui --features frame-diagnostics`                                                        | `PASS`                      | 0.40 s，RSS 189,704 KiB；`.tmp/gpui-refactor/phase-1/check-frame-diagnostics.log`                                                                                                                                                                                               |
| `cargo test --locked -p gpui`                                                                                      | `PASS`                      | 214 unit + 1 integration，0 failed                                                                                                                                                                                                                                              |
| `cargo test --locked -p gpui --features frame-diagnostics`                                                         | `PASS`                      | 227+ tests，含 frame lifecycle 与 skipped-submission test                                                                                                                                                                                                                       |
| `./script/clippy -p gpui --features frame-diagnostics`                                                             | `PASS`                      | all-features release clippy，含 philosophy check                                                                                                                                                                                                                                |
| `git diff --check`                                                                                                 | `PASS`                      | `.tmp/gpui-refactor/phase-1/diff-check.log`                                                                                                                                                                                                                                     |
| `cargo test --locked -p gpui --features frame-diagnostics frame_diagnostics_runner -- --nocapture`                 | `PASS`                      | 100 root-dirty + 100 cached-panel + 100 input frames；draw p50/p95/p99 = 72,307/92,273/99,438 ns；input-to-present = 105,760/118,914/125,356 ns；100 prepaint + 100 paint cache replays；0 dropped events；raw log in `.tmp/gpui-refactor/phase-1/frame-diagnostics-runner.log` |
| `cargo fmt --all -- --check`                                                                                       | `FAIL (baseline)`           | 仅剩 `crates/grammars/vendor/tree-sitter-typst/benches/bench_main.rs` 的既有排序漂移；本阶段文件已单独格式化                                                                                                                                                                    |
| EXP-001                                                                                                            | `PASS`                      | production Editor workload overhead ≤1.902%；candidate/baseline p95/p99 均低于 1.05×；0 skipped/dropped                                                                                                                                                                         |
| EXP-002                                                                                                            | `PASS`                      | Editor ratios ≤1.0064；tabs cached/dirty 1.000898/1.001593；Settings cached/dirty 1.017539/0.990516；command palette 1.024921；operation/cache counts 全部相同                                                                                                                  |
| EXP-011                                                                                                            | `PASS`                      | feature-off allocation 不高于 baseline；diagnostics 增量 ≤0.003 alloc/frame、≤0.203% bytes/frame；peak RSS ≤1.011×                                                                                                                                                              |
| `frame_allocations` off/on probe                                                                                   | `PASS (allocation portion)` | `cargo test --features test-support[,frame-diagnostics] --test frame_allocations`；raw logs `.tmp/gpui-refactor/phase-1/frame-allocations-{disabled,enabled}.log`；warm-up 后两种模式的分配数/字节相同                                                                          |
| production Editor runner，feature off/on，各 7 个 CPU-pinned process                                               | `PASS`                      | median overhead：cached 0.31%、typing 0.49%、scroll 0.62%、resize 1.90%；`.tmp/gpui-refactor/phase-1/editor-workload-summary.txt`                                                                                                                                               |
| production Editor candidate vs `567ff2f4`，feature on/off                                                          | `PASS`                      | p95/p99 全部低于 1.05×；exact input-to-present p50/p95/p99 为 2.913/3.057/3.299 ms，0 skipped/dropped                                                                                                                                                                           |
| production Editor allocation/RSS                                                                                   | `PASS`                      | feature-off allocation 与 baseline 相同或更低；diagnostics 增量 ≤0.003 alloc/frame、≤0.203% bytes/frame；peak RSS 1.011×                                                                                                                                                        |
| `cargo test --locked -p gpui --lib --features frame-diagnostics printable_text_input_is_linked_to_presented_frame` | `PASS`                      | printable text input provenance reaches the presented frame                                                                                                                                                                                                                     |
| GPUI 235-test suite，`frame-diagnostics,accessibility`，连续 20 次                                                 | `PASS`                      | 4,700 tests，0 failed；source-scoped collector 无跨 context 串扰                                                                                                                                                                                                                |
| workspace tabs / Settings / command palette，各 7 个 CPU-pinned baseline/candidate process                         | `PASS`                      | 所有 median per-frame p95 ratio <1.05；operation/cache replay counts 完全相同；`.tmp/gpui-refactor/phase-1/exp002-product-summary.txt`                                                                                                                                          |

上游 A/B/C 审查（均基于 `decbf641b18f1982b3475c037e7c5c554471574f` 之后的
live `FETCH_HEAD=a84689073d296dfd39987bc7dd478e43ef76d83a`；未 cherry-pick）：

| Upstream                                   | Class | Disposition                                                                                                                                                       |
| ------------------------------------------ | ----- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `a9e7b7767207ccf40bcba04a4c5cad96d6916f13` | A     | Already equivalent: ZZZ already has feature-gated `InputLatencyTracker` and public snapshot on `Window`.                                                          |
| `9e236090b9a31338caf233d440f724922b58d7e1` | C     | Its shipped behavior is coupled to Zed telemetry and `input_latency_ui`; the local diagnostics contract is implemented independently without importing telemetry. |
| `a21007b7a948e46be719150f5e9968bfcd1078`   | C     | Broad profiler/window rewrite with unisolatable Zed feature and consumer changes; no clean current ZZZ caller for the full patch.                                 |
| `07a0bd1220bddb940028d610e40319a459dd49fb` | C     | Product keybindings and debug overlay are outside the frame journal contract.                                                                                     |
| `1861e58f984c76afc06032e753557994ffc8fe44` | C     | Hang journal, watchdog, and telemetry migration are outside this stage and would import a rejected product surface.                                               |
| `55007f518bc1d49e6b3291c5eaa1aabf649b36fd` | C     | Dirty-to-present reporting is tied to Zed's telemetry report; the local collector remains provider-free.                                                          |
| `cd4fc8de4ca8548cca2567352b87bcaaec13328f` | C     | Platform frame-request timestamp APIs touch platform seams reserved for later stages and require the unabsorbed hang journal.                                     |
| `36b6d0951fdee409f0957294a69360ba2e8e980e` | C     | Follow-up to the rejected/unabsorbed hang journal path; no isolated ZZZ invariant to retain at this point.                                                        |

The local implementation is a small B-style architectural port of the safe
frame-observation invariant, but it does not copy upstream code; the table records
the source decisions required by the absorbing-upstream workflow. No upstream local
commit was created for this stage.

提交：core `8d410f4b55cc77ef30cba627aff0dcfc326cc308`；runner
`d031aa92deb7547e56d671420d8772f08434e167`；cached replay
`766b7cb85cd58c35a56b3182208f5e6c2bd6cf40`；batched journal/allocation probe
`567ff2f4c39edb84a60e99dfebbd6033c50ce6e3`；submission outcome/skip coverage
`1520da4179`、`147871401a`；production Editor runner `602836a9f5`、`3e5b9e6c1f`、
`7b420641d5`；printable input provenance `9ec0114958`；window-instance collector
`dfcff1a2cd`；product workload `19f075920a`。

EXP-001、EXP-002 与 EXP-011 已完成并通过。EXP-002 使用
`567ff2f4c3` 作为 owner 拆分前的 instrumentation baseline；三个产品 workload 的
median per-frame p95 ratio 均低于 1.05，且 phase operation/cache replay count 没有
变化。阶段 8 的数据门现已打开。默认产品构建继续不启用 diagnostics feature。

### 阶段 2：并发与 accessibility 边界

状态：`COMPLETE`（macOS/Windows native runtime 为 `NOT RUN`）

已完成：

- 按 `8886dcb0d4ea0e145e4512d415d3260602eca99` 的 B 级安全部分，在现有 ZZZ
  `PlatformDispatcher` 和 priority queue 上增加 `ThreadedDispatcher`。
- worker pool、main-thread handoff、real-time timer queue、idle tracking 和显式
  dispatcher drop shutdown 已实现；普通 deterministic tests 继续使用 `TestDispatcher`。
- 3 个 focused tests 通过，其中 teardown test 创建并销毁 100 个 dispatcher 实例。
- worker/timer runnable panic 使用 unwind guard 隔离，panic 后 idle tracking 保持一致；
  Task drop cancellation 不遗留 in-flight work。
- `BenchAppContext::threaded` 可显式选择 production-like dispatcher；默认构造仍使用
  deterministic virtual-clock `TestDispatcher`。
- 增加可选 `accessibility` feature 的 AccessKit core seam：稳定 node ID、完整
  `TreeUpdate` snapshot、focus 映射和一次性 action router；不导入上游 writer state。
- 按 `1d029c5ff5654fb1b1e8caf4462993c8ee13a133` 的 B 类安全子集，把
  element-derived stable ID、physical bounds、nested semantic tree 接入实际
  `Element::prepaint` 和 completed `BuiltFrame`，并分别接入 Linux、macOS 和 Windows
  native adapter。没有复制 writer state 或 synthetic children。
- `Div` 提供 feature-gated role、label 与 action listener；action map 属于 completed
  frame，`Window::dispatch_accessibility_action` 每次只调用一个 `(node, action)` handler。
  accessibility feature 开启时完整重建 semantic tree，避免 cached view 产生不完整树；
  默认构建继续使用原 cache path，因此没有 accessibility runtime 开销。
- AccessKit nodes now advertise registered actions and support feature-gated disabled,
  selected, expanded, toggled and value properties. GPUI re-exports core AccessKit types
  only when `accessibility` is enabled.
- `ui` adds an opt-in `accessibility` feature. Button/ButtonLike/IconButton, Tab,
  ListItem and TreeViewItem emit semantic roles and state; Button accessibility Click is
  routed through the same callback as mouse activation. A real `ui` integration snapshot
  covers roles, labels, state and one-shot action dispatch.
- `AccessibilityBridge` 通过 thread-safe activation/action/deactivation callbacks 连接
  native adapter，所有 action 回到 foreground `Window` 后只派发一次，不让平台线程
  直接重入 entity update。
- X11/Wayland 使用 `accesskit_unix`，macOS 使用 `SubclassingAdapter`，Windows 处理
  `WM_GETOBJECT` 与 UI Automation focus event；macOS teardown 在 renderer 前释放 adapter，
  避免 native view ownership cycle。
- 桌面 `zzz` 按 target 启用 accessibility；Web 仍保持 unsupported 且不会启用该 feature。
- Button、InputField、Editor、List/Tree、Tabs、Modal/Dialog 和 status notification 均有
  role/name/state snapshot；`track_focus` 与 AccessKit Focus action 共享同一 `FocusHandle`。

阶段 2 上游 A/B/C 决策：

| Upstream                                   | Class | Disposition                                                                                                                                                                                       |
| ------------------------------------------ | ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `8886dcb0d4ea0e145e4512d415d3260602eca99`  | B     | Ported the isolated threaded worker/timer/main-handoff behavior to ZZZ names and queue APIs; omitted upstream BenchDispatcher rename, benchmark feature graph, and unrelated benchmark reporting. |
| `1d029c5ff5654fb1b1e8caf4462993c8ee13a133` | B     | Ported core semantics, callback/action routing, and Linux/macOS/Windows adapters onto the local completed-frame and platform-capability seams; omitted writer state and synthetic children.       |
| `cc053a4a6fa2fd0e8793201ed9099466af1be0b1` | C     | AccessKit writer/semantic tree is absent from ZZZ; the one-line author-id builder cannot be isolated from the missing accessibility chain.                                                        |
| `0eda7703f6c88aa08a25c1d2105ff1ca46f775d4` | B     | Reclassified after the macOS adapter arrived; retained only adapter-first teardown to break the native view ownership cycle.                                                                      |

EXP-008 已达标。EXP-006 的 core、action、focus、named controls、Linux native adapter 和
AT-SPI/Orca enumeration 已通过；macOS VoiceOver 与 Windows Narrator runtime 只能在对应
主机执行，按完成合同保留为 `NOT RUN`，不是未完成的核心代码迁移。

阶段 2 验证：

| 命令或检查                                                                                                 | 结果   | 原始数据/说明                                                                                                                                                                                  |
| ---------------------------------------------------------------------------------------------------------- | ------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cargo check --locked -p gpui --features test-support`                                                     | `PASS` | ThreadedDispatcher port                                                                                                                                                                        |
| `cargo test --locked -p gpui --features test-support threaded_dispatcher`                                  | `PASS` | handoff、real-time timer/cancel、100 次 dispatcher teardown                                                                                                                                    |
| `./script/clippy -p gpui --features test-support`                                                          | `PASS` | all-target release clippy + philosophy                                                                                                                                                         |
| `cargo check --locked -p gpui --features accessibility`                                                    | `PASS` | AccessKit 0.24.1 core seam                                                                                                                                                                     |
| `cargo test --locked -p gpui --features accessibility accessibility`                                       | `PASS` | 3 semantic snapshot/action tests                                                                                                                                                               |
| `cargo test --locked -p gpui --lib --features accessibility,frame-diagnostics`                             | `PASS` | 232 tests passed，含 nested element prepaint tree、action dispatch 与完整 cached-frame rebuild                                                                                                 |
| `./script/clippy -p gpui --features accessibility`                                                         | `PASS` | all-target/all-feature release clippy 与 philosophy gate 通过                                                                                                                                  |
| `cargo test --locked -p ui --features accessibility --test accessibility`                                  | `PASS` | Button/Tab/List/Tree/Dialog/Status roles, labels, state, Click 与 Focus action                                                                                                                 |
| `./script/clippy -p ui --features accessibility`                                                           | `PASS` | UI accessibility feature all-target release clippy + philosophy gate                                                                                                                           |
| `cargo test --locked -p ui_input --features accessibility input_field_emits_text_input_semantics`          | `PASS` | TextInput role/name/focus snapshot                                                                                                                                                             |
| `cargo test --locked -p editor --features accessibility test_editor_accessibility_semantics`               | `PASS` | MultilineTextInput role/name/focus；原 IME/focus suites 继续通过                                                                                                                               |
| `cargo test --locked -p gpui_linux --features accessibility`                                               | `PASS` | 26 tests；X11/Wayland capability matrices declare native adapter                                                                                                                               |
| macOS `cargo check --locked ... --target x86_64-apple-darwin --features accessibility`                     | `PASS` | adapter 与 teardown cross-compile；runtime `NOT RUN`                                                                                                                                           |
| Windows tests cross-check，`--target x86_64-pc-windows-gnu --no-default-features --features accessibility` | `PASS` | `WM_GETOBJECT` adapter/test code cross-compiles；runtime `NOT RUN`                                                                                                                             |
| Linux isolated app + pyatspi tree/action + `orca --list-apps`                                              | `PASS` | `zzz` registered as AT-SPI application；35 nodes；native click action returned true；`.tmp/gpui-refactor/phase-2/linux-{atspi-smoke,orca-list-apps}.log`                                       |
| EXP-008 100-seed parity/hang/leak gate                                                                     | `PASS` | 100 seeds，background→main、timer、cancellation、panic cleanup、window teardown 全通过；0 hang/failure；warm-up 后 mean 10.686 ms、CV 2.134%；raw log `.tmp/gpui-refactor/phase-2/exp-008.log` |

提交：ThreadedDispatcher core `49b351afb1edab173ca46dd073c663b44aabbf14`；
AccessKit semantic core `a7745abdefe9d3e2cebb33482aac35d5e6a0289f`；EXP-008
BenchAppContext/panic follow-up `41f6fccba156b795d49b8564bbe357712f9b2aa5`；element
prepaint semantics `83fe764989`；completed tree/action routing `03342d511d`；action
advertisement `d530264622`；semantic state properties `e662afa392`；UI component
semantics `e7f4ccb52a`、`57b1caf569`；callback seam `cbdad449a5`；Linux adapter
`dd9a27221d`；macOS adapter/teardown `db6cb81aeb`；Windows adapter `4c00dbf000`；
desktop enablement `bdd5e7ee26`；focus mapping `863014f6a9`；Dialog/Status
`3b894eb92b`；InputField `2e01ae3f69`；Editor `5bf1569ff1`。

### 阶段 3：真实 headless renderer

状态：`COMPLETE`（Windows runtime 为 `NOT RUN`）

已完成：

- `gpui_wgpu` 增加 surface-free `WgpuContext::new_headless`、offscreen texture、padded
  readback buffer 和 `WgpuHeadlessRenderer`；窗口 swapchain 继续走原有 surface path。
- `gpui_platform::current_headless_renderer` 在 Linux/FreeBSD/Windows test-support
  下返回 WGPU renderer；macOS 继续使用 Metal headless renderer。
- 同一 primitive corpus（quad/border/shadow/underline/monochrome text/SVG atlas、
  polychrome image/emoji atlas、path）在 RADV hardware 与 llvmpipe fallback 各连续
  运行 100 次，无 hang 或初始化失败。
- corpus 已补齐显式 clip boundary，并在 1x、2x 分别运行；对初始 headless commit
  `0347f6196e` 和当前实现使用同一测试 harness 后，hardware/fallback 四组 PNG 的
  pre/post-refactor SHA-256 均完全相同。

EXP-003 当前结果：

| 检查                                                                                                       | 结果               | 证据                                                                                                                  |
| ---------------------------------------------------------------------------------------------------------- | ------------------ | --------------------------------------------------------------------------------------------------------------------- |
| gpui-ce reference `headless_primitives`                                                                    | `FAIL (reference)` | 当前 RADV 主机 4 tests 中 3 passed；`smoothed_primitives_share_one_contour` pixel assertion failed；未放宽阈值        |
| `cargo test --locked -p gpui_wgpu --features test-support --test headless_renderer`                        | `PASS`             | hardware + fallback each 100 runs；`.tmp/gpui-refactor/phase-3/headless-renderer.log`                                 |
| Hardware/fallback pixel comparison                                                                         | `PASS`             | 54/20,000 differing pixels = 0.27%；max channel delta 1；≤0.5% threshold                                              |
| Hardware pixel artifact                                                                                    | `PASS`             | `.tmp/gpui-refactor/phase-3/hardware.png`, SHA-256 `a153441213a6a9626d05669c516ec876a269b29d58af3d92bd0daeaf8362a658` |
| Fallback pixel artifact                                                                                    | `PASS`             | `.tmp/gpui-refactor/phase-3/fallback.png`, SHA-256 `07be370bfc32685d553602de7e6d7a3394aa17d8a272b94e12912dad18cf1ab8` |
| 1x/2x per-adapter pre/post-refactor golden                                                                 | `PASS`             | RADV 与 llvmpipe 各自 1x/2x hash 全部相同；artifacts in `.tmp/gpui-refactor/phase-3/{baseline,current}-1x-2x/`        |
| `cargo test --locked -p gpui_wgpu --features test-support`                                                 | `PASS`             | 15 unit + 2 headless integration tests                                                                                |
| `./script/clippy -p gpui_wgpu --features test-support`                                                     | `PASS`             | release/all-target checks + philosophy                                                                                |
| `cargo test --locked -p gpui_platform --features test-support`                                             | `PASS`             | platform factory returns real renderer on Linux                                                                       |
| Windows `cargo check --locked -p gpui_wgpu --tests --target x86_64-pc-windows-gnu --features test-support` | `PASS`             | 同一 headless runner cross-compiles                                                                                   |
| Windows hardware/software adapter runtime                                                                  | `NOT RUN`          | 当前主机无法执行；保留同一 test command 给 Windows QA                                                                 |

一次 hardware-vs-llvmpipe 2x 比较得到 1.1375% 像素差异；该比较混合两个 adapter，
不符合 EXP-003 的 per-platform baseline 定义，记录在
`.tmp/gpui-refactor/phase-3/headless-1x-2x-comparison-repeats.log` 作为 `INVALID SAMPLE`，
没有放宽 0.5% 阈值。正确的同 adapter、同 scale、pre/post-refactor 比较为 0 差异。

EXP-003 在当前 Linux 主机达标。Windows runtime 需要目标主机，在 Windows PowerShell
运行以下命令；测试会分别选择普通 adapter 与 `force_fallback_adapter`，各循环 100 次：

```powershell
cargo test --locked -p gpui_wgpu --features test-support --test headless_renderer -- --nocapture
```

预期两个 tests 均通过，无 hang，并把同 adapter、同 scale 的输出与批准 baseline 比较。
该 runtime 项标记 `NOT RUN`，不阻塞当前主机完成阶段 3。

提交：renderer core `0347f6196e`；platform factory `b28b235a09`；1x/2x golden gate
`aae393b8c4`。

### 阶段 4：拆分 `Window`

状态：`IN PROGRESS`

4A frame owner 当前进度：

- 新增内部 `frame.rs`，迁入 `WindowInvalidator`、dirty views/update count、draw phase、
  platform waker、`FrameBuildId` lifecycle 和 frame diagnostics event batching。
- `Window` 继续持有并委托 `WindowInvalidator`；公开 invalidate/draw/present API 未变化。
- `Frame`、`DeferredDraw`、`PrepaintStateIndex`、`PaintIndex` 及 cache range 状态已迁入
  `frame.rs`；`Window` 只保留绘制 orchestration 和必要的 `pub(crate)` owner 边界。
- 新增 `FrameScheduler`，集中持有 next-frame callbacks、expanded dirty views、present
  demand 和 full-refresh state；`Window` 与 `App` 只通过 owner 方法协调这些状态。
- 第一、第二提交都只移动状态和私有算法，不改变行为；不可变 `BuiltFrame` 尚未迁移。

验证：

| 命令或检查                                                       | 结果   | 证据                                                    |
| ---------------------------------------------------------------- | ------ | ------------------------------------------------------- |
| `cargo check --locked -p gpui`                                   | `PASS` | 默认配置编译通过                                        |
| `cargo check --locked -p gpui --features frame-diagnostics`      | `PASS` | diagnostics 配置编译通过                                |
| `cargo test --locked -p gpui --lib`                              | `PASS` | 218 tests passed                                        |
| `cargo test --locked -p gpui --lib --features frame-diagnostics` | `PASS` | 220 tests passed，含 frame diagnostics runner/lifecycle |
| `git diff --check`                                               | `PASS` | frame/window 迁移无 whitespace error                    |

提交：invalidation owner `1946811f30`；completed frame state owner `ccd2a038df`；frame
scheduler state `9317625c82`。
下一步：继续 4B，先抽出 hitbox、dispatch tree、focus/tab、pointer capture 和 key/action
routing owner，并固定 routing order 回归测试。

4B interaction owner 当前进度：

- 新建内部 `interaction.rs`，由 `InteractionOwner` 持有 rendered/next frame、hitbox ID
  分配、命中测试结果和 pointer capture；`Window` 仅协调绘制和输入生命周期。
- GPUI element、deferred draw、image、anchored UI 和 test context 的内部访问已切换到
  interaction owner；公开 `Window`、`Hitbox`、focus 和 pointer capture API 保持兼容。
- 本提交只收拢所有权和字段路径，没有改变 capture、hit-test、dispatch tree 或 cache replay
  算法；focus state、cursor requests 和 key/action routing method 已开始下沉，仍待完整
  action/pointer routing coverage。
- key、action 与 mouse capture/bubble 顺序均有固定 snapshot；pointer capture 在 mouse-up
  后自动释放也有独立回归测试。
- mouse listener capture/bubble、active drag cleanup、pointer capture release 与 action
  capture/bubble 算法已迁入 `InteractionOwner`；`Window` 只保留 event normalization、
  inspector/diagnostics 协调和公开 façade。
- key/modifier capture/bubble、multi-stroke timeout/replay、focus/tab 与 pending-input cleanup
  已迁入 `InteractionOwner`；platform input handler 的查询和文本提交通过窄 helper 保留原
  take/set 生命周期。
- pointer position 与 tooltip identity/bounds 已迁入 `InteractionOwner`，Window 不再保存
  平行 pointer/tooltip 状态。

验证：

| 命令或检查                                                       | 结果   | 证据                                                                   |
| ---------------------------------------------------------------- | ------ | ---------------------------------------------------------------------- |
| `cargo check --locked -p gpui`                                   | `PASS` | interaction owner 默认配置编译通过                                     |
| `cargo test --locked -p gpui --lib`                              | `PASS` | 223 tests passed，含 key/action/mouse routing snapshots                |
| `cargo test --locked -p gpui --lib --features frame-diagnostics` | `PASS` | 225 tests passed，含 diagnostics lifecycle/runner 与 routing snapshots |
| `./script/clippy -p gpui --features frame-diagnostics`           | `PASS` | all-target release clippy 与 philosophy gate 通过                      |
| `git diff --check`                                               | `PASS` | interaction owner 迁移无 whitespace error                              |

提交：frame/input owner `a53e41c9ef`；focus/input state owner `785c0f8c2c`；routing
order snapshot `1c52ff05bd`；hitbox/cursor owner methods `9f4d68603c`。
完整 routing snapshots `487347fcff`。
pointer/action routing owner `d52a99235d`。
key routing owner `8a2ce07544`；focus/pending-input owner `0ea05bb762`。
pointer position `3cb415ee42`；tooltip state `be55f5df44`。
下一步：收敛 4D completed-frame accessibility/diagnostics payload，并继续检查
`window.rs` 中仍可归属 owner 的平行状态。

4C text input owner 当前进度：

- 新建内部 `text_input.rs` 与 `TextInputOwner`，将 frame cache 中的
  `PlatformInputHandler` rendered/next handler 列表从 `Frame` 移出，保持 cache range
  索引和 handler drop/cancellation 语义。
- 增加窄 `TextInputClient` candidate-geometry seam；UTF-16 selection、marked text 和
  mutation 继续由现有 `InputHandler` compatibility adapter 提供，未引入平台层
  `AsyncWindowContext` 到应用代码。
- 原 `PlatformInputHandler` adapter 已正式改名为 `TextInputClient`；Linux、macOS、
  Windows、Web、TestWindow 与 Vim test 均只依赖 client 名称。旧名称只保留一个公开
  type alias，并明确在下一个 GPUI breaking release 删除。
- `invalidate_character_coordinates` 通过 owner 读取 IME candidate bounds；公开
  `Window::handle_input` 和 Editor UTF-16/multi-cursor 行为保持不变。

验证：

| 命令或检查                                                                                                     | 结果   | 证据                                                       |
| -------------------------------------------------------------------------------------------------------------- | ------ | ---------------------------------------------------------- |
| `cargo check --locked -p gpui`                                                                                 | `PASS` | text input owner 默认配置编译通过                          |
| `cargo test --locked -p gpui --lib input`                                                                      | `PASS` | pending input handler tests 2 passed                       |
| `cargo test --locked -p gpui --lib --features frame-diagnostics input`                                         | `PASS` | same 2 tests passed with diagnostics                       |
| `cargo check --locked -p gpui -p gpui_platform -p gpui_linux -p gpui_macos -p gpui_windows -p gpui_web -p vim` | `PASS` | 所有 platform bridge 与 compatibility alias 编译通过       |
| `cargo test --locked -p vim test_helix_jump_consumes_label_keystrokes_before_ime`                              | `PASS` | native printable key 与 pending keymap precedence 保持不变 |
| `./script/clippy -p gpui --features frame-diagnostics`                                                         | `PASS` | all-target release clippy 与 philosophy gate 通过          |
| `git diff --check`                                                                                             | `PASS` | text input owner 迁移无 whitespace error                   |

提交：handler cache owner `71c66cc2ae`；narrow client seam `34694640b2`；cache-slot swap
fix `ff126abc7a`；platform client boundary `ad83ac2e12`。
下一步：完成 native platform IME repetition runbook；当前 Linux 单元层的 Editor IME、
UTF-16、多 cursor 和 candidate geometry checks 已通过。

4D built frame 当前进度：

- 增加只读 `BuiltFrame` projection，包含 `Scene`、interaction snapshot、text-input
  snapshot、accessibility update placeholder 和 diagnostics snapshot。
- platform `draw` 与 test-support `render_to_image` 现在只接收 completed frame 的不可变
  scene view；frame owner 继续负责构建、交换和 cache replay。
- 这是 4D 的第一步，snapshot 的 accessibility/diagnostics 数据接线和 renderer contract
  仍待阶段 5；当前不宣称 immutable ownership 已完全收敛。
- `AccessibilityUpdate` 与 diagnostics snapshot 现在由 completed `Frame` 持有，随 frame
  swap 进入 `BuiltFrame`；frame clear 会清空旧 payload，避免复用上一帧数据。
- `Window.pending_frame_timing` 平行状态已删除；`present` 从 `BuiltFrame.diagnostics` 读取
  当前 build timing，accessibility bridge 从同一 completed frame 读取 semantic update。
- accessibility semantic core 的非空 update 已通过 frame build/swap 测试；native adapter
  仍未接入，本提交没有复制 Zed writer 或平台 adapter 代码。

验证：

| 命令或检查                                                                 | 结果   | 证据                                                |
| -------------------------------------------------------------------------- | ------ | --------------------------------------------------- |
| `cargo check --locked -p gpui`                                             | `PASS` | built frame projection 编译通过                     |
| `cargo test --locked -p gpui --lib`                                        | `PASS` | 223 tests passed                                    |
| `cargo test --locked -p gpui --lib --features frame-diagnostics`           | `PASS` | 225 tests passed                                    |
| `cargo test --locked -p gpui --lib --features accessibility accessibility` | `PASS` | 5 tests passed，含 completed-frame semantic payload |
| `./script/clippy -p gpui --features frame-diagnostics`                     | `PASS` | all-target release clippy 与 philosophy gate 通过   |
| `git diff --check`                                                         | `PASS` | built frame migration 无 whitespace error           |

提交：BuiltFrame projection `e321e6fa0b`；render contract adapter `32d3b403a8`；
completed-frame payload `8897cb326c`。
下一步：审查并收敛剩余 completed-frame/read-only renderer contract，再按 EXP-004/005
决定是否抽出 `gpui_render` crate。

### 阶段 5：render contract 与 WGPU 模块化

状态：`COMPLETE`（Windows hardware runtime 为 `NOT RUN`）

已完成的第一步：

- 在 `gpui/src/render_api.rs` 建立内部 `RenderScene`、`RenderTarget`、`Renderer` 和
  `FrameSubmission` contract。
- 提供 `submit_compat` adapter，保留现有 `PlatformWindow::draw(&Scene)` backend path；
  `Window::present` 现在提交 `BuiltFrame.scene` 的只读视图。
- 没有引入 `App`、`Entity` 或公开 `Window` 到 render contract；没有改变 WGPU/Metal
  swapchain 行为。
- `gpui_wgpu/src/wgpu_renderer/resources.rs` 现在承载 GPU resource lifetime，
  `pipelines.rs` 承载 pipeline/layout definitions，`surface.rs` 承载 surface config/context
  alias，`frame.rs` 承载 upload/readback POD，`drawing.rs` 承载 primitive/path draw calls；
  headless path 保持独立。
- `PlatformRenderTarget::draw` 与 render contract 传播真实 submission outcome；headless
  discard、surface/device recovery 和 DirectX error 不再被 `Window::present` 记作成功
  present，frame journal 以 `SubmissionSkipped` 记录未提交帧。

验证：

| 命令或检查                                                                                                                          | 结果   | 证据                                                                                                            |
| ----------------------------------------------------------------------------------------------------------------------------------- | ------ | --------------------------------------------------------------------------------------------------------------- |
| `cargo check --locked -p gpui`                                                                                                      | `PASS` | render contract 与 compatibility adapter 编译通过                                                               |
| `CARGO_TARGET_DIR=.tmp/gpui-refactor/phase-5/target-gpui cargo check --locked -p gpui`                                              | `PASS` | clean 18.63 s，RSS 860,532 KiB；baseline 19.52 s                                                                |
| `CARGO_TARGET_DIR=.tmp/gpui-refactor/phase-5/target-gpui cargo check --locked -p gpui_platform -p gpui_wgpu -p gpui_linux`          | `PASS` | clean 17.13 s，RSS 858,320 KiB                                                                                  |
| `cargo test --locked -p gpui --lib window::tests::test_frame_waker_fires_on_frame_demand`                                           | `PASS` | completed frame submit path 通过                                                                                |
| `cargo test --locked -p gpui --lib --features frame-diagnostics window::tests::test_frame_diagnostics_follow_build_through_present` | `PASS` | diagnostics present path 通过                                                                                   |
| `cargo test --locked -p gpui_wgpu --lib`                                                                                            | `PASS` | 15 WGPU unit tests passed                                                                                       |
| `cargo test --locked -p gpui_wgpu --features test-support --test headless_renderer`                                                 | `PASS` | hardware/fallback primitive corpus passed                                                                       |
| drawing split 后 fallback adapter corpus                                                                                            | `PASS` | llvmpipe corpus passed                                                                                          |
| drawing split 后 hardware adapter corpus                                                                                            | `PASS` | 当前主机重新暴露 RADV；hardware/fallback 各连续渲染 100 次，17 tests 全通过                                     |
| EXP-004 paired clean `cargo check --locked -p gpui`                                                                                 | `PASS` | `e321e6fa0b` baseline median 19.84 s；`cee87f046e` candidate median 19.80 s；ratio 0.998；各 3 个独立 target    |
| EXP-004 dependency/source audit                                                                                                     | `PASS` | no normal dependency cycle；0 product source edits；`gpui_platform` downstream count 8，未增加                  |
| EXP-005 `release-fast` binary size                                                                                                  | `PASS` | file 5,339,438,400 vs 5,339,134,088 bytes（1.000057×）；ELF total 271,792,740 vs 271,657,972 bytes（1.000496×） |
| `cargo test --locked -p gpui --lib --features frame-diagnostics,accessibility`                                                      | `PASS` | 231 tests passed，含 submission-result contract 与 completed-frame diagnostics                                  |
| `cargo test --locked -p gpui_wgpu --features test-support`                                                                          | `PASS` | 15 unit + hardware/fallback 各 100-run headless integration tests                                               |
| Wayland/Vulkan `failed_frames_release_surface_images --ignored`                                                                     | `PASS` | 连续 8 次 acquire 后失败均释放 swapchain image，随后健康帧成功 present                                          |
| Windows `cargo check --locked -p gpui_wgpu --tests --target x86_64-pc-windows-gnu --features test-support`                          | `PASS` | renderer/headless tests cross-compile                                                                           |
| `./script/clippy -p gpui_wgpu --features test-support`                                                                              | `PASS` | WGPU all-target release clippy 与 philosophy gate 通过                                                          |
| `./script/clippy -p gpui --features frame-diagnostics`                                                                              | `PASS` | all-target release clippy 与 philosophy gate 通过                                                               |
| `git diff --check`                                                                                                                  | `PASS` | render contract migration 无 whitespace error                                                                   |

提交：render contract adapter `32d3b403a8`；resource split `f0284a014f`；pipeline split
`90e8f62f3e`；surface split `bffcfd6e62`；frame helper split `ea29cf0afc`；drawing split
`cee87f046e`；submission outcome `1520da4179`。
EXP-004/005 当前状态：`PASS`。compatibility façade 没有产品 consumer edit；依赖图无
cycle；clean build、binary size 和 golden pixel 均在预算内。尽管实验允许继续抽 crate，
`gpui_wgpu` 仍直接依赖 GPUI 的 text、geometry、atlas 和 image types；单独创建
`gpui_render` 不会缩小当前 rebuild graph，反而会增加第二个 public ownership boundary。
因此本阶段明确拒绝新增 crate，长期保留 `gpui::render_api` 内部模块；若未来先拆出真正
独立的 shared render data model，再重新评估。原始复测日志为
`.tmp/gpui-refactor/phase-5/exp004-recheck-{baseline,candidate}-{1,2,3}.log` 和
`.tmp/gpui-refactor/phase-5/exp005-current-release-fast.log`。

surface recovery test 首轮在第三次失败帧发现 swapchain image exhaustion；修复后失败路径
会 drop acquired view/frame 并 reconfigure surface，8 次连续失败后健康帧成功 present。
来源：gpui-ce `c6b17e616a35271183ab49f0da1890ee81953a99` 的
`crates/gpui_wgpu/src/wgpu_renderer/surface_tests.rs`，Apache-2.0，按当前 renderer API
改造。提交：`2d31da2bbd`。Windows hardware runtime 按阶段 3 runbook 标记 `NOT RUN`。

### 阶段 6：platform capability 与 lifecycle

状态：`COMPLETE`

已完成的第一步：

- `PlatformWindow` 增加可查询的 `PlatformCapabilities`，覆盖 text input、accessibility、
  headless renderer、frame callbacks 和 window controls。
- capability matrix 单独声明 native IME candidate-position 能力；Web、Linux headless 和
  TestWindow 的 unsupported 状态不再进入静默 `update_ime_position` 调用。
- system-bell 和 request-attention 也进入 capability matrix；公开 Window façade 在调用
  bell、attention、minimize、maximize 或 fullscreen 前检查对应能力。
- native prompt 进入 capability matrix；Linux、Web 和 headless backend 直接选择 GPUI
  rendered prompt，不再先调用空的 native prompt 实现。
- clipboard 支持由 `ClipboardCapabilities` 区分同步 read 与 write；`Application`/`App`
  均可查询。Web 明确为 write-only，Linux headless 为 unsupported；Web clipboard Promise
  rejection、缺失 browser window 和非文本写入均会记录错误，不再静默丢弃。
- interactive move/resize 进入 `WindowControls`，公开 façade 只调用 backend 声明支持的
  操作；Web/headless/Windows 不再接收无效果的 resize/move 请求。
- 默认 capability 明确为 unsupported，避免 backend 未实现时静默声称支持；公开
  `Window::platform_capabilities` façade 保持 additive、无 consumer 修改。
- `WindowServiceCapabilities` 覆盖 decorations、input region、client inset、app ID、
  document metadata、character palette、simple fullscreen、system tabs 和 titlebar action；
  公开 façade 在调用 backend 前查询能力，不再依赖对应 trait 的静默 no-op。
- 增加 Linux 测试锁定默认 capability matrix 的显式 unsupported 语义。
- Linux X11、Wayland、headless、TestWindow、macOS、Windows 和 Web backend 均显式声明
  capability matrix；桌面 adapter 启用 accessibility，headless/Web 明确 unsupported。
- 新增 `TextInputBridge` supertrait，将 input handler ownership 与 IME candidate position
  从宽 `PlatformWindow` trait 抽离；所有 backend 已迁移，`PlatformWindow` 继续作为
  composite façade，因此现有调用语义和公开 surface 不变。
- 新增 `InputSource` supertrait，将 mouse position、modifiers、Caps Lock 和 input callback
  registration 从 `PlatformWindow` 抽离；各 backend 仅移动原实现，dispatch order 与
  callback ownership 不变。
- 新增 `WindowHost` supertrait，将 frame waker、request-frame callback 与
  completed-frame lifecycle 从 `PlatformWindow` 抽离；Wayland/Web completion 语义和
  TestWindow demand/waker 行为保持原实现。
- `WindowHost` 随后接管通用 window geometry、resize/scale/display、activation/title、
  fullscreen/minimize/zoom 和 status/lifecycle callbacks；`PlatformWindow` 现在主要保留
  平台专属扩展、capabilities 与 test downcast façade。
- 新增 `SystemServices` supertrait，将 native prompt 与 system bell 从
  `PlatformWindow` 抽离；Linux/Web/headless 的 rendered-prompt fallback、macOS/Windows
  native prompt 和各平台 bell 行为均保留。
- 新增 `AppLifecycle` supertrait，将 run/quit/restart、activation/hide 和 quit/reopen
  callbacks 从宽 `Platform` 抽离；Test/visual、Linux、macOS、Windows、Web event-loop
  实现均只做所有权移动。
- `AppLifecycleCapabilities` 现在区分 external event loop、quit、restart、activate、hide
  和 hide-other-apps；Web 的 unsupported lifecycle 调用会明确记录 warning，Windows 的
  hide-other-apps 路径不再触发 `unimplemented!()`。
- `run_embedded` 的 TestPlatform integration test 锁定 launch callback、外部 update 重入
  与 handle-drop 生命周期；Web 明确声明 external-event-loop ownership。
- 新增 `AccessibilityBridge` supertrait；所有 backend 显式实现，未支持平台对非空
  semantic update 返回 error，避免 silent no-op。
- 新增 `RendererFactory` contract；`gpui_platform::current_headless_renderer` 继续作为
  compatibility façade，并委托 current-platform factory 创建 Metal 或 WGPU offscreen
  renderer。
- 新增 `PlatformRenderTarget` supertrait，将 scene draw、atlas、subpixel capability、
  GPU specs 和 test readback 从 `PlatformWindow` 抽离；render contract 的 compatibility
  adapter 仍通过 composite façade 提交同一 `Scene`。

当前矩阵：

| Backend               | Text input | IME position | Native prompt | Clipboard | Accessibility | System bell | Offscreen/headless window render | Frame callbacks | Window controls                                  |
| --------------------- | ---------- | ------------ | ------------- | --------- | ------------- | ----------- | -------------------------------- | --------------- | ------------------------------------------------ |
| TestWindow            | yes        | no           | yes           | R/W       | feature       | no          | runtime renderer dependent       | yes             | fullscreen + move                                |
| Linux X11             | yes        | yes          | rendered      | R/W       | yes           | yes         | no                               | yes             | full desktop set                                 |
| Linux Wayland         | yes        | yes          | rendered      | R/W       | yes           | runtime     | no                               | yes             | compositor dependent + move/resize; no attention |
| Linux headless window | no         | no           | rendered      | none      | no            | no          | no; scene is discarded           | no              | fullscreen state only                            |
| macOS                 | yes        | yes          | native        | R/W       | yes           | yes         | test-support only                | yes             | desktop set + move; no interactive resize        |
| Windows               | yes        | yes          | native        | R/W       | yes           | yes         | test-support only                | yes             | desktop set; no interactive move/resize          |
| Web                   | no         | no           | rendered      | write     | no            | no          | no                               | yes             | fullscreen only                                  |

验证：

| 命令或检查                                                                                                                   | 结果                        | 证据                                                      |
| ---------------------------------------------------------------------------------------------------------------------------- | --------------------------- | --------------------------------------------------------- |
| `cargo check --locked -p gpui`                                                                                               | `PASS`                      | capability façade 编译通过                                |
| `cargo test --locked -p gpui --lib default_platform_capabilities_are_explicitly_unsupported`                                 | `PASS`                      | capability default test passed                            |
| `cargo test --locked -p gpui --lib test_platform_capability_matrix`                                                          | `PASS`                      | TestWindow capability matrix passed                       |
| `cargo test --locked -p gpui_linux --lib capability_matrix`                                                                  | `PASS`                      | X11/Wayland/headless matrices, 3 passed                   |
| `cargo test --locked -p gpui --lib`                                                                                          | `PASS`                      | 225 tests，含 `run_embedded` ownership                    |
| `cargo check --locked -p gpui_macos --tests --target x86_64-apple-darwin`                                                    | `PASS`                      | macOS capability tests cross-compile                      |
| `cargo check --locked -p gpui_windows --tests --target x86_64-pc-windows-gnu --no-default-features --features accessibility` | `PASS`                      | capability/adapter tests cross-compile；runtime `NOT RUN` |
| `RUSTC_BOOTSTRAP=1 cargo check --locked -p gpui_web --tests --target wasm32-unknown-unknown`                                 | `PASS`                      | workaround for `wasm_thread` nightly-only feature         |
| `cargo test --locked -p gpui --lib --features frame-diagnostics,accessibility`                                               | `PASS`                      | 231 tests passed after render/capability changes          |
| `cargo test --locked -p gpui --lib`                                                                                          | `PASS`                      | 223 tests passed after platform splits                    |
| `cargo test --locked -p gpui --lib input`                                                                                    | `PASS`                      | 2 pending-input/handler tests passed                      |
| `cargo test --locked -p gpui --lib interactive`                                                                              | `PASS`                      | key/action/mouse routing tests, 5 passed                  |
| `cargo test --locked -p gpui --lib --features accessibility accessibility`                                                   | `PASS`                      | semantic/action/bridge tests, 4 passed                    |
| `cargo test --locked -p gpui_platform --features test-support`                                                               | `PASS`                      | renderer factory returns real Linux renderer              |
| `./script/clippy -p gpui_platform --features test-support`                                                                   | `PASS`                      | renderer factory contract passes release clippy           |
| `cargo check --locked -p gpui_windows -p gpui_macos -p gpui_web`                                                             | `PASS (host package check)` | target runtime/tests cannot execute on Linux              |
| `./script/clippy -p gpui --features frame-diagnostics`                                                                       | `PASS`                      | all-target release clippy 与 philosophy gate 通过         |
| `./script/clippy -p gpui_linux`                                                                                              | `PASS`                      | Linux all-target release clippy 与 philosophy gate 通过   |
| `git diff --check`                                                                                                           | `PASS`                      | platform capability change 无 whitespace error            |

提交：capability façade `982cb1642a`；backend matrices `5d77a17d79`；text input bridge
`efd05dd0cb`；input source `7c1e5e0de2`；window host `07b4298de0`；system services
`9c8de23b7c`；app lifecycle `1ff5d99840`；accessibility bridge `42a8e9765d`；renderer
factory `85fffe8fe5`；platform render target `c3f2d1bb90`；completed window host
`2b2d467ce5`；IME position capability `c7d7aac2d5`；optional operation gating
`3ef63e8c00`；desktop assertion coverage `fd778d5672`；embedded lifecycle test
`a37079c358`；native prompt capability `ff2ae9aaf3`；clipboard capability
`7542cae1ec`；Web clipboard errors `2cb4199d55`；interactive move/resize gating
`13a7611025`；lifecycle capability `93aa67658d`；optional window services
`dc14ed987e`。阶段 6 已覆盖 frame lifecycle、IME、clipboard、window controls、native
accessibility、`run_embedded` 和外部 event loop；每个 backend matrix 都明确区分
supported 与 unsupported。

### 阶段 7：UI 集成边界

状态：`IN PROGRESS`

已完成的第一步：

- 删除 `ui_input::ERASED_EDITOR_FACTORY` process-global `OnceLock`。
- 增加 per-App `ErasedEditorFactory` global registration；Editor 在 app 初始化时显式注册，
  InputField、Picker 和 RemoteConnection 通过当前 App 的 adapter 读取。
- `ui_prompt` renderer 不再读取 `WorkspaceSettings` 或订阅 product settings；system prompt
  policy 由 `zzz` app composition 决定，generic renderer 只负责 prompt view。
- 保留 `ErasedEditor` façade 和 Editor UTF-16/IME 行为，未引入跨 crate product API。
- `create_editor`/`InputField::try_new` 提供显式 factory-missing error；兼容
  `create_editor_or_fallback` 集中记录 error 并返回可 focus、可程序化读写的 inert
  editor。`InputField`、`Picker` 和 `RemoteConnectionPrompt` 均通过该入口构造，已不存在
  绕过错误边界的 `cx.global::<ErasedEditorFactory>()` 调用。
- 原 `component` Cargo package 已改名为 `ui_component_registry`，明确它只承载 preview
  metadata、registry 和 example layout；`component_preview` 继续承载产品级 workspace
  preview UI。源码目录暂留 `crates/component` 以避免无行为收益的文件搬迁，但 workspace
  dependency 和 Rust crate path 已不存在含糊的 `component` 名称。

验证：

| 命令或检查                                                                                                              | 结果                      | 证据                                                                                                                                                                                                |
| ----------------------------------------------------------------------------------------------------------------------- | ------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cargo check --locked -p ui_input -p editor -p picker -p remote_connection`                                             | `PASS`                    | app-scoped factory consumers 编译通过                                                                                                                                                               |
| `cargo check --locked -p ui_input -p picker -p remote_connection`                                                       | `PASS`                    | 所有 factory consumer 通过统一的非 panic 构造边界                                                                                                                                                   |
| `cargo check --locked -p ui_prompt -p zzz`                                                                              | `PASS`                    | prompt policy moved to app composition                                                                                                                                                              |
| `cargo test --locked -p ui_input --lib`                                                                                 | `PASS`                    | 1 startup-order/fallback test passed                                                                                                                                                                |
| `./script/clippy -p ui_input`                                                                                           | `PASS`                    | all-target release clippy 与 philosophy gate 通过                                                                                                                                                   |
| `./script/clippy -p ui_input -p picker -p remote_connection`                                                            | `PASS`                    | all-target/all-feature release clippy 与 philosophy gate 通过                                                                                                                                       |
| `./script/clippy -p ui_prompt`                                                                                          | `PASS`                    | all-target release clippy 与 philosophy gate 通过                                                                                                                                                   |
| `cargo test --locked -p editor ime`                                                                                     | `PASS`                    | 6 IME/composition tests passed                                                                                                                                                                      |
| `cargo test --locked -p editor focus`                                                                                   | `PASS`                    | 2 focus tests passed                                                                                                                                                                                |
| `cargo test --locked -p editor input`                                                                                   | `PASS`                    | 8 UTF-16/multi-cursor input tests passed                                                                                                                                                            |
| `cargo check --locked -p ui_component_registry -p ui_macros -p ui -p ui_input -p component_preview -p workspace -p zzz` | `PASS`                    | registry rename 的 13 个直接 workspace consumer 全部编译通过                                                                                                                                        |
| `cargo test --locked -p ui_component_registry -p ui_macros -p component_preview --lib`                                  | `PASS`                    | 4 tests passed，0 failed                                                                                                                                                                            |
| `./script/clippy -p ui_component_registry -p ui_macros -p component_preview`                                            | `PASS`                    | all-target/all-feature release clippy 与 philosophy gate 通过                                                                                                                                       |
| EXP-010 editor-only incremental `cargo check --locked -p zzz -v`                                                        | `PASS`                    | `editor` 与 56 个真实 downstream crate 重编译；`ui`、`ui_input`、`ui_component_registry`、`component_preview` 均为 `Fresh`；4.78 s；`.tmp/gpui-refactor/phase-7/exp010-editor-only-incremental.log` |
| `git diff --check`                                                                                                      | `PASS`                    | ui_input boundary change 无 whitespace error                                                                                                                                                        |
| `cargo test --locked -p agent_ui --lib`                                                                                 | `FAIL (baseline overlap)` | 293 passed；6 agent action/focus failures remain; representative baseline reproduction is recorded in `.tmp/gpui-refactor/phase-9/baseline-agent-ui-form-tab.log`                                   |

提交：app-scoped editor adapter `f4f22a68fc`；prompt policy/renderer split `d90153f6cc`；
non-panicking factory boundary `d296469604`；all-consumer factory fallback `ca93ff5625`；
preview registry rename `7cb589a3ba`。

EXP-010 当前状态：`PASS`。process-global factory 已删除；3 个 factory consumer 通过统一
构造入口且 startup-order fallback test 通过；prompt product policy 已迁出 generic crate；
editor-only 增量构建保持 generic `ui`、`ui_input`、registry 和 preview crate 为 fresh。

下一步：完成 EXP-009 的平台 IME 重复运行与 native runbook，并在最终 Linux app smoke
中覆盖实际 UI startup。

### 阶段 8：invalidation 实验

状态：`COMPLETE — REJECTED`

`FrameDirtyReason` 现在记录 invalidation 的最早受影响 phase：`Layout`、`Prepaint`、
`Paint` 或 `Accessibility`。现有 `cx.notify()` 和 `Window::refresh()` 都明确记录
`Layout`，但仍执行原完整 invalidation；默认构建不包含这些 diagnostics 字段。

EXP-007 使用临时、未提交的 `notify_with` 原型，在 button、tab、list item、tree item、
text input、dialog、status、toolbar、scrollbar 和 editor viewport 十类代表性 cached view
上测试跳过 dirty-view rebuild：

- accessibility 关闭时，component render work 从 10 降到 0，表面下降 100%，但 10/10
  debug/visual projection 保留旧 state，mouse input 也调用旧 handler；correctness gate
  立即失败。
- accessibility 开启时，GPUI 为保证完整 semantic tree 会禁用 cached replay；full 与
  scoped 两组 component render work 都是 200，下降 0%，未达到 20% 阈值。
- 因此不存在同时达到 phase-work 阈值且保持 visual/input/accessibility 行为的候选。
  focus、IME 和 native adapter 无需继续冒险验证，因为 visual/input 已经明确不等价。

临时 `notify_with`、cache-policy 分支和实验 view 已全部删除；产品代码没有 scoped
invalidation API、双实现或 compatibility path，完整 `cx.notify()` 语义保持不变。
仅保留 phase dirty diagnostics。

验证：

| 命令或检查                                                                                                     | 结果       | 证据                                                                         |
| -------------------------------------------------------------------------------------------------------------- | ---------- | ---------------------------------------------------------------------------- |
| `cargo test --locked -p gpui --lib --features frame-diagnostics scoped_invalidation_experiment -- --nocapture` | `REJECTED` | 100% work drop，但 10/10 stale views 且 stale input handlers                 |
| 同一实验加 `accessibility` feature                                                                             | `REJECTED` | semantic rebuild required；0% work drop                                      |
| 临时 prototype 删除后 `cargo test --locked -p gpui --lib --features frame-diagnostics,accessibility`           | `PASS`     | 235 tests passed                                                             |
| `./script/clippy -p gpui --features frame-diagnostics`                                                         | `PASS`     | all-target/all-feature release clippy 与 philosophy gate                     |
| 原始实验输出                                                                                                   | `PASS`     | `.tmp/gpui-refactor/phase-8/exp007-scoped-invalidation{,-accessibility}.log` |

提交：phase dirty diagnostics `28203d1737`。EXP-007 最终决策：`REJECTED`。

### 阶段 9：收敛与最终验证

状态：`NOT STARTED`

最新 workspace validation：`cargo test --workspace --locked --no-fail-fast` 完成 workspace
编译，但测试阶段包含已在计划 baseline 复现的 agent_ui action/focus failures，并有多个
editor formatter/inlay tests 长时间运行；为避免无界 session 已中断。精确记录见
`.tmp/gpui-refactor/phase-9/workspace-test-summary.txt`。最终阶段仍需在收敛后重新运行
并区分 baseline failure、environment hang 和真实回归。

## Upstream A/B/C 记录

`ThreadedDispatcher` 已按 `8886dcb0` 做 B 类移植；AccessKit core、action routing 与
Linux/macOS/Windows adapter 已按 `1d029c5f` 分开移植。`0eda7703` 在 macOS adapter
到位后重分类为 B，仅保留 teardown 修复；`cc053a4a` 的缺失 writer follow-up 保持 C。
完整决策见 `upstream-sync-2026-10-03-gpui.md`。

## 外部平台 QA

| 平台/检查                | 当前状态  | 说明                                                 |
| ------------------------ | --------- | ---------------------------------------------------- |
| Linux runtime/headless   | `PASS`    | isolated XDG app startup、RADV 与 AT-SPI tree/action |
| macOS runtime/VoiceOver  | `NOT RUN` | 最终提供 exact runbook                               |
| Windows runtime/Narrator | `NOT RUN` | 最终提供 exact runbook                               |
| Linux Orca               | `PASS`    | `orca --list-apps` 识别 `zzz`；pyatspi action 可执行 |

### Native accessibility runbook

macOS 14+ / VoiceOver：

```sh
cargo check --locked -p gpui_macos --tests --target x86_64-apple-darwin --features accessibility
cargo run --locked -p zzz
open -a VoiceOver
```

在 VoiceOver rotor 中确认 ZZZ window 下可以找到 Button、Text Input、Editor、List、Tree、
Tab、Dialog 和 Status；键盘移动 focus 后 VoiceOver focus 跟随；分别对 Button Click 与
Text Input Focus 执行一次 action，确认只触发一次。结束后运行：

```sh
osascript -e 'tell application "VoiceOver" to quit'
```

Windows 11 / Narrator（PowerShell）：

```powershell
cargo check --locked -p gpui_windows --tests --target x86_64-pc-windows-msvc --features accessibility
cargo run --locked -p zzz
Start-Process narrator.exe
```

用 Narrator scan mode 检查同一组 role/name/state，确认键盘 focus 与 Narrator focus 一致，
Button Click 与 Text Input Focus 各执行一次且没有重复。结束后运行：

```powershell
Stop-Process -Name Narrator
```
