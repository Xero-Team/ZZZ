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
| 当前阶段                   | 阶段 6/7：platform 与 UI 边界              |
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

提交：`6c44518908f6fb461f7cec7598fac87a623e6db8`。

下一步：建立 feature-gated frame journal、统一 `FrameBuildId`、collector/snapshot
API 和 EXP-001/002/011 runner，并测量 disabled/enabled overhead。

### 阶段 1：frame diagnostics 与预算

状态：`IN PROGRESS`

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

当前仍待完成：

- EXP-001 的 input-to-present p50/p95/p99、skip/coalesce runner；
- EXP-002 的 cached/dirty phase cost runner；
- EXP-011 的 production-like steady-state allocation/peak RSS probe与 <=2% timing gate；
- disabled/enabled instrumentation overhead 的正式 workload 采样。

阶段 1 验证：

| 命令或检查                                                                                         | 结果                        | 原始数据/说明                                                                                                                                                                                                                                                                   |
| -------------------------------------------------------------------------------------------------- | --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cargo check --locked -p gpui`                                                                     | `PASS`                      | 0.40 s，RSS 189,612 KiB；`.tmp/gpui-refactor/phase-1/check-default.log`                                                                                                                                                                                                         |
| `cargo check --locked -p gpui --features frame-diagnostics`                                        | `PASS`                      | 0.40 s，RSS 189,704 KiB；`.tmp/gpui-refactor/phase-1/check-frame-diagnostics.log`                                                                                                                                                                                               |
| `cargo test --locked -p gpui`                                                                      | `PASS`                      | 214 unit + 1 integration，0 failed                                                                                                                                                                                                                                              |
| `cargo test --locked -p gpui --features frame-diagnostics`                                         | `PASS`                      | 215 unit + 1 integration，0 failed；含 frame lifecycle test                                                                                                                                                                                                                     |
| `./script/clippy -p gpui --features frame-diagnostics`                                             | `PASS`                      | all-features release clippy，含 philosophy check                                                                                                                                                                                                                                |
| `git diff --check`                                                                                 | `PASS`                      | `.tmp/gpui-refactor/phase-1/diff-check.log`                                                                                                                                                                                                                                     |
| `cargo test --locked -p gpui --features frame-diagnostics frame_diagnostics_runner -- --nocapture` | `PASS`                      | 100 root-dirty + 100 cached-panel + 100 input frames；draw p50/p95/p99 = 72,307/92,273/99,438 ns；input-to-present = 105,760/118,914/125,356 ns；100 prepaint + 100 paint cache replays；0 dropped events；raw log in `.tmp/gpui-refactor/phase-1/frame-diagnostics-runner.log` |
| `cargo fmt --all -- --check`                                                                       | `FAIL (baseline)`           | 仅剩 `crates/grammars/vendor/tree-sitter-typst/benches/bench_main.rs` 的既有排序漂移；本阶段文件已单独格式化                                                                                                                                                                    |
| EXP-001                                                                                            | `PARTIAL`                   | deterministic input-to-present runner now samples p50/p95/p99; platform frame skip and 10-second editor scroll workload remain                                                                                                                                                  |
| EXP-002                                                                                            | `PARTIAL`                   | runner emits layout/prepaint/paint totals and 100 cached prepaint/paint replays; editor/product corpus and p95 comparison remain                                                                                                                                                |
| EXP-011                                                                                            | `PARTIAL`                   | allocation probe matches off/on after warm-up: 12/25 allocations and 6,504/13,104 bytes per cached/dirty frame; synthetic timing overhead is ~10.5% cached and ~3.9% dirty, so the <=2% production-workload gate remains open                                                   |
| `frame_allocations` off/on probe                                                                   | `PASS (allocation portion)` | `cargo test --features test-support[,frame-diagnostics] --test frame_allocations`；raw logs `.tmp/gpui-refactor/phase-1/frame-allocations-{disabled,enabled}.log`；warm-up 后两种模式的分配数/字节相同                                                                          |

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
`567ff2f4c39edb84a60e99dfebbd6033c50ce6e3`。

下一步：把 runner 接到 Editor 100k-line/scroll workload，复跑 EXP-001/002/011 的正式
阈值比较，并记录 production-like timing 与 peak RSS；默认产品构建继续不启用
diagnostics feature。

### 阶段 2：并发与 accessibility 边界

状态：`IN PROGRESS`

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
  `TreeUpdate` snapshot、focus 映射和一次性 action router；不连接平台 writer。

阶段 2 上游 A/B/C 决策：

| Upstream                                   | Class | Disposition                                                                                                                                                                                       |
| ------------------------------------------ | ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `8886dcb0d4ea0e145e4512d415d3260602eca99`  | B     | Ported the isolated threaded worker/timer/main-handoff behavior to ZZZ names and queue APIs; omitted upstream BenchDispatcher rename, benchmark feature graph, and unrelated benchmark reporting. |
| `cc053a4a6fa2fd0e8793201ed9099466af1be0b1` | C     | AccessKit writer/semantic tree is absent from ZZZ; the one-line author-id builder cannot be isolated from the missing accessibility chain.                                                        |
| `0eda7703f6c88aa08a25c1d2105ff1ca46f775d4` | C     | ZZZ has no macOS AccessKit adapter or SubclassingAdapter ownership to release.                                                                                                                    |

当前仍待完成：GPUI semantic node/action 与 Window/Element 集成、Linux/Windows/macOS
adapter 分离审查，以及 native screen-reader runbook。ThreadedDispatcher EXP-008 已
达标；AccessKit EXP-006 尚未运行。

后续边界进展：`AccessibilityBridge` 现在接收 backend-neutral `AccessibilityUpdate`；
空更新在所有 backend 可通过，非空 semantic update 在未接 native adapter 时返回明确
unsupported error。没有导入缺失的 AccessKit writer/adapter 路径。

阶段 2 验证：

| 命令或检查                                                                | 结果      | 原始数据/说明                                                                                                                                                                                  |
| ------------------------------------------------------------------------- | --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cargo check --locked -p gpui --features test-support`                    | `PASS`    | ThreadedDispatcher port                                                                                                                                                                        |
| `cargo test --locked -p gpui --features test-support threaded_dispatcher` | `PASS`    | handoff、real-time timer/cancel、100 次 dispatcher teardown                                                                                                                                    |
| `./script/clippy -p gpui --features test-support`                         | `PASS`    | all-target release clippy + philosophy                                                                                                                                                         |
| `cargo check --locked -p gpui --features accessibility`                   | `PASS`    | AccessKit 0.24.1 core seam                                                                                                                                                                     |
| `cargo test --locked -p gpui --features accessibility accessibility`      | `PASS`    | 3 semantic snapshot/action tests                                                                                                                                                               |
| EXP-006 native adapter/screen-reader QA                                   | `NOT RUN` | ZZZ 尚无 AccessKit platform adapter；保留精确 runbook 待 adapter 阶段                                                                                                                          |
| EXP-008 100-seed parity/hang/leak gate                                    | `PASS`    | 100 seeds，background→main、timer、cancellation、panic cleanup、window teardown 全通过；0 hang/failure；warm-up 后 mean 10.686 ms、CV 2.134%；raw log `.tmp/gpui-refactor/phase-2/exp-008.log` |

提交：ThreadedDispatcher core `49b351afb1edab173ca46dd073c663b44aabbf14`；
AccessKit semantic core `a7745abdefe9d3e2cebb33482aac35d5e6a0289f`；EXP-008
BenchAppContext/panic follow-up `41f6fccba1edab173ca46dd073c663b44aabbf14`。

### 阶段 3：真实 headless renderer

状态：`IN PROGRESS`

已完成：

- `gpui_wgpu` 增加 surface-free `WgpuContext::new_headless`、offscreen texture、padded
  readback buffer 和 `WgpuHeadlessRenderer`；窗口 swapchain 继续走原有 surface path。
- `gpui_platform::current_headless_renderer` 在 Linux/FreeBSD/Windows test-support
  下返回 WGPU renderer；macOS 继续使用 Metal headless renderer。
- 同一 primitive corpus（quad/border/shadow/underline/monochrome text/SVG atlas、
  polychrome image/emoji atlas、path）在 RADV hardware 与 llvmpipe fallback 各连续
  运行 100 次，无 hang 或初始化失败。

EXP-003 当前结果：

| 检查                                                                                | 结果               | 证据                                                                                                                  |
| ----------------------------------------------------------------------------------- | ------------------ | --------------------------------------------------------------------------------------------------------------------- |
| gpui-ce reference `headless_primitives`                                             | `FAIL (reference)` | 当前 RADV 主机 4 tests 中 3 passed；`smoothed_primitives_share_one_contour` pixel assertion failed；未放宽阈值        |
| `cargo test --locked -p gpui_wgpu --features test-support --test headless_renderer` | `PASS`             | hardware + fallback each 100 runs；`.tmp/gpui-refactor/phase-3/headless-renderer.log`                                 |
| Hardware/fallback pixel comparison                                                  | `PASS`             | 54/20,000 differing pixels = 0.27%；max channel delta 1；≤0.5% threshold                                              |
| Hardware pixel artifact                                                             | `PASS`             | `.tmp/gpui-refactor/phase-3/hardware.png`, SHA-256 `a153441213a6a9626d05669c516ec876a269b29d58af3d92bd0daeaf8362a658` |
| Fallback pixel artifact                                                             | `PASS`             | `.tmp/gpui-refactor/phase-3/fallback.png`, SHA-256 `07be370bfc32685d553602de7e6d7a3394aa17d8a272b94e12912dad18cf1ab8` |
| `cargo test --locked -p gpui_wgpu --features test-support`                          | `PASS`             | 15 unit + 2 headless integration tests                                                                                |
| `./script/clippy -p gpui_wgpu --features test-support`                              | `PASS`             | release/all-target checks + philosophy                                                                                |
| `cargo test --locked -p gpui_platform --features test-support`                      | `PASS`             | platform factory returns real renderer on Linux                                                                       |
| Windows hardware/software adapter runtime                                           | `NOT RUN`          | 当前主机无法执行；保留同一 test command 给 Windows QA                                                                 |

当前仍待完成：明确 1x/2x font-dependent text/emoji/SVG golden policy、Windows 同 corpus
runtime，以及将 Linux pixel artifacts 纳入后续 renderer/Window migration regression gate。
因此 EXP-003 在当前 Linux 主机的 primitive/readback 部分达标，阶段 3 保持
`IN PROGRESS`，不声称跨平台完成。

提交：renderer core `0347f6196e`；platform factory `b28b235a09`。

### 阶段 4：拆分 `Window`

状态：`IN PROGRESS`

4A frame owner 当前进度：

- 新增内部 `frame.rs`，迁入 `WindowInvalidator`、dirty views/update count、draw phase、
  platform waker、`FrameBuildId` lifecycle 和 frame diagnostics event batching。
- `Window` 继续持有并委托 `WindowInvalidator`；公开 invalidate/draw/present API 未变化。
- `Frame`、`DeferredDraw`、`PrepaintStateIndex`、`PaintIndex` 及 cache range 状态已迁入
  `frame.rs`；`Window` 只保留绘制 orchestration 和必要的 `pub(crate)` owner 边界。
- 第一、第二提交都只移动状态和私有算法，不改变行为；不可变 `BuiltFrame` 尚未迁移。

验证：

| 命令或检查                                                       | 结果   | 证据                                                    |
| ---------------------------------------------------------------- | ------ | ------------------------------------------------------- |
| `cargo check --locked -p gpui`                                   | `PASS` | 默认配置编译通过                                        |
| `cargo check --locked -p gpui --features frame-diagnostics`      | `PASS` | diagnostics 配置编译通过                                |
| `cargo test --locked -p gpui --lib`                              | `PASS` | 218 tests passed                                        |
| `cargo test --locked -p gpui --lib --features frame-diagnostics` | `PASS` | 220 tests passed，含 frame diagnostics runner/lifecycle |
| `git diff --check`                                               | `PASS` | frame/window 迁移无 whitespace error                    |

提交：invalidation owner `1946811f30`；completed frame state owner `ccd2a038df`。
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
下一步：继续 4B，将 key/action/mouse dispatch methods 本身下沉到 interaction owner，
然后收敛 4C/4D ownership。

4C text input owner 当前进度：

- 新建内部 `text_input.rs` 与 `TextInputOwner`，将 frame cache 中的
  `PlatformInputHandler` rendered/next handler 列表从 `Frame` 移出，保持 cache range
  索引和 handler drop/cancellation 语义。
- 增加窄 `TextInputClient` candidate-geometry seam；UTF-16 selection、marked text 和
  mutation 继续由现有 `InputHandler` compatibility adapter 提供，未引入平台层
  `AsyncWindowContext` 到应用代码。
- `invalidate_character_coordinates` 通过 owner 读取 IME candidate bounds；公开
  `Window::handle_input` 和 Editor UTF-16/multi-cursor 行为保持不变。

验证：

| 命令或检查                                                             | 结果   | 证据                                              |
| ---------------------------------------------------------------------- | ------ | ------------------------------------------------- |
| `cargo check --locked -p gpui`                                         | `PASS` | text input owner 默认配置编译通过                 |
| `cargo test --locked -p gpui --lib input`                              | `PASS` | pending input handler tests 2 passed              |
| `cargo test --locked -p gpui --lib --features frame-diagnostics input` | `PASS` | same 2 tests passed with diagnostics              |
| `./script/clippy -p gpui --features frame-diagnostics`                 | `PASS` | all-target release clippy 与 philosophy gate 通过 |
| `git diff --check`                                                     | `PASS` | text input owner 迁移无 whitespace error          |

提交：handler cache owner `71c66cc2ae`；narrow client seam `34694640b2`；cache-slot swap fix `ff126abc7a`.
下一步：运行 Editor IME、UTF-16、多 cursor 和 candidate geometry checks，再进入 4D
immutable `BuiltFrame`。

4D built frame 当前进度：

- 增加只读 `BuiltFrame` projection，包含 `Scene`、interaction snapshot、text-input
  snapshot、accessibility update placeholder 和 diagnostics snapshot。
- platform `draw` 与 test-support `render_to_image` 现在只接收 completed frame 的不可变
  scene view；frame owner 继续负责构建、交换和 cache replay。
- 这是 4D 的第一步，snapshot 的 accessibility/diagnostics 数据接线和 renderer contract
  仍待阶段 5；当前不宣称 immutable ownership 已完全收敛。
- `BuiltFrame.accessibility` 已替换为真实 `AccessibilityUpdate` contract，并在 present
  时提交给 platform bridge；当前尚无 native adapter，因此 frame payload 仍为空。

验证：

| 命令或检查                                                       | 结果   | 证据                                              |
| ---------------------------------------------------------------- | ------ | ------------------------------------------------- |
| `cargo check --locked -p gpui`                                   | `PASS` | built frame projection 编译通过                   |
| `cargo test --locked -p gpui --lib`                              | `PASS` | 219 tests passed                                  |
| `cargo test --locked -p gpui --lib --features frame-diagnostics` | `PASS` | 221 tests passed                                  |
| `./script/clippy -p gpui --features frame-diagnostics`           | `PASS` | all-target release clippy 与 philosophy gate 通过 |
| `git diff --check`                                               | `PASS` | built frame migration 无 whitespace error         |

提交：BuiltFrame projection `e321e6fa0b`；render contract adapter `32d3b403a8`。
下一步：补齐 BuiltFrame 的 accessibility/diagnostics payload 和 read-only renderer contract，
再按 EXP-004/005 决定是否抽出 `gpui_render` crate。

### 阶段 5：render contract 与 WGPU 模块化

状态：`IN PROGRESS`

已完成的第一步：

- 在 `gpui/src/render_api.rs` 建立内部 `RenderScene`、`RenderTarget`、`Renderer` 和
  `FrameSubmission` contract。
- 提供 `submit_compat` adapter，保留现有 `PlatformWindow::draw(&Scene)` backend path；
  `Window::present` 现在提交 `BuiltFrame.scene` 的只读视图。
- 没有引入 `App`、`Entity` 或公开 `Window` 到 render contract；没有改变 WGPU/Metal
  swapchain 行为。
- `gpui_wgpu/src/wgpu_renderer/resources.rs` 现在承载 GPU resource lifetime，
  `pipelines.rs` 承载 pipeline/layout definitions，`surface.rs` 承载 surface config/context
  alias，`frame.rs` 承载 upload/readback POD；headless path 保持独立。

验证：

| 命令或检查                                                                                                                          | 结果   | 证据                                                   |
| ----------------------------------------------------------------------------------------------------------------------------------- | ------ | ------------------------------------------------------ |
| `cargo check --locked -p gpui`                                                                                                      | `PASS` | render contract 与 compatibility adapter 编译通过      |
| `CARGO_TARGET_DIR=.tmp/gpui-refactor/phase-5/target-gpui cargo check --locked -p gpui`                                              | `PASS` | clean 18.63 s，RSS 860,532 KiB；baseline 19.52 s       |
| `CARGO_TARGET_DIR=.tmp/gpui-refactor/phase-5/target-gpui cargo check --locked -p gpui_platform -p gpui_wgpu -p gpui_linux`          | `PASS` | clean 17.13 s，RSS 858,320 KiB                         |
| `cargo test --locked -p gpui --lib window::tests::test_frame_waker_fires_on_frame_demand`                                           | `PASS` | completed frame submit path 通过                       |
| `cargo test --locked -p gpui --lib --features frame-diagnostics window::tests::test_frame_diagnostics_follow_build_through_present` | `PASS` | diagnostics present path 通过                          |
| `cargo test --locked -p gpui_wgpu --lib`                                                                                            | `PASS` | 15 WGPU unit tests passed                              |
| `cargo test --locked -p gpui_wgpu --features test-support --test headless_renderer`                                                 | `PASS` | hardware/fallback primitive corpus passed              |
| `./script/clippy -p gpui_wgpu --features test-support`                                                                              | `PASS` | WGPU all-target release clippy 与 philosophy gate 通过 |
| `./script/clippy -p gpui --features frame-diagnostics`                                                                              | `PASS` | all-target release clippy 与 philosophy gate 通过      |
| `git diff --check`                                                                                                                  | `PASS` | render contract migration 无 whitespace error          |

提交：render contract adapter `32d3b403a8`；resource split `f0284a014f`；pipeline split `90e8f62f3e`；surface split `bffcfd6e62`；frame helper split `ea29cf0afc`.
EXP-004/005 当前状态：`PARTIAL`。clean check 在当前样本中未超过 baseline*1.10，
但 consumer edit/rebuild graph、binary-size 和 golden scene equivalence 尚未完成；因此
不创建独立 `gpui_render` crate。

下一步：把 `gpui_wgpu` 的 frame/drawing/headless 按 contract 拆分，补齐 EXP-004/005
consumer/build graph 和 pixel evidence；只有达标才评估独立 `gpui_render` crate。

### 阶段 6：platform capability 与 lifecycle

状态：`IN PROGRESS`

已完成的第一步：

- `PlatformWindow` 增加可查询的 `PlatformCapabilities`，覆盖 text input、accessibility、
  headless renderer、frame callbacks 和 window controls。
- 默认 capability 明确为 unsupported，避免 backend 未实现时静默声称支持；公开
  `Window::platform_capabilities` façade 保持 additive、无 consumer 修改。
- 增加 Linux 测试锁定默认 capability matrix 的显式 unsupported 语义。
- Linux X11、Wayland、headless、TestWindow、macOS、Windows 和 Web backend 均显式声明
  capability matrix；未接 native AccessKit adapter 的平台统一声明 accessibility false。
- 新增 `TextInputBridge` supertrait，将 input handler ownership 与 IME candidate position
  从宽 `PlatformWindow` trait 抽离；所有 backend 已迁移，`PlatformWindow` 继续作为
  composite façade，因此现有调用语义和公开 surface 不变。
- 新增 `InputSource` supertrait，将 mouse position、modifiers、Caps Lock 和 input callback
  registration 从 `PlatformWindow` 抽离；各 backend 仅移动原实现，dispatch order 与
  callback ownership 不变。
- 新增 `WindowHost` supertrait，将 frame waker、request-frame callback 与
  completed-frame lifecycle 从 `PlatformWindow` 抽离；Wayland/Web completion 语义和
  TestWindow demand/waker 行为保持原实现。
- 新增 `SystemServices` supertrait，将 native prompt 与 system bell 从
  `PlatformWindow` 抽离；Linux/Web/headless 的 rendered-prompt fallback、macOS/Windows
  native prompt 和各平台 bell 行为均保留。
- 新增 `AppLifecycle` supertrait，将 run/quit/restart、activation/hide 和 quit/reopen
  callbacks 从宽 `Platform` 抽离；Test/visual、Linux、macOS、Windows、Web event-loop
  实现均只做所有权移动。
- 新增 `AccessibilityBridge` supertrait；所有 backend 显式实现，未支持平台对非空
  semantic update 返回 error，避免 silent no-op。
- 新增 `RendererFactory` contract；`gpui_platform::current_headless_renderer` 继续作为
  compatibility façade，并委托 current-platform factory 创建 Metal 或 WGPU offscreen
  renderer。

当前矩阵：

| Backend               | Text input | Accessibility | Offscreen/headless window render | Frame callbacks | Window controls       |
| --------------------- | ---------- | ------------- | -------------------------------- | --------------- | --------------------- |
| TestWindow            | yes        | no            | runtime renderer dependent       | yes             | fullscreen only       |
| Linux X11             | yes        | no            | no                               | yes             | full desktop set      |
| Linux Wayland         | yes        | no            | no                               | yes             | compositor dependent  |
| Linux headless window | no         | no            | no; scene is discarded           | no              | fullscreen state only |
| macOS                 | yes        | no            | test-support only                | yes             | full desktop set      |
| Windows               | yes        | no            | test-support only                | yes             | full desktop set      |
| Web                   | no         | no            | no                               | yes             | fullscreen only       |

验证：

| 命令或检查                                                                                   | 结果                        | 证据                                                    |
| -------------------------------------------------------------------------------------------- | --------------------------- | ------------------------------------------------------- |
| `cargo check --locked -p gpui`                                                               | `PASS`                      | capability façade 编译通过                              |
| `cargo test --locked -p gpui --lib default_platform_capabilities_are_explicitly_unsupported` | `PASS`                      | capability default test passed                          |
| `cargo test --locked -p gpui --lib test_platform_capability_matrix`                          | `PASS`                      | TestWindow capability matrix passed                     |
| `cargo test --locked -p gpui_linux --lib capability_matrix`                                  | `PASS`                      | X11/Wayland/headless matrices, 3 passed                 |
| `cargo test --locked -p gpui --lib`                                                          | `PASS`                      | 221 tests passed after TextInputBridge split            |
| `cargo test --locked -p gpui --lib input`                                                    | `PASS`                      | 2 pending-input/handler tests passed                    |
| `cargo test --locked -p gpui --lib interactive`                                              | `PASS`                      | input/action routing tests, 3 passed                    |
| `cargo test --locked -p gpui --lib --features accessibility accessibility`                   | `PASS`                      | semantic/action/bridge tests, 4 passed                  |
| `cargo test --locked -p gpui_platform --features test-support`                               | `PASS`                      | renderer factory returns real Linux renderer            |
| `./script/clippy -p gpui_platform --features test-support`                                   | `PASS`                      | renderer factory contract passes release clippy         |
| `cargo check --locked -p gpui_windows -p gpui_macos -p gpui_web`                             | `PASS (host package check)` | target runtime/tests cannot execute on Linux            |
| `./script/clippy -p gpui --features frame-diagnostics`                                       | `PASS`                      | all-target release clippy 与 philosophy gate 通过       |
| `./script/clippy -p gpui_linux`                                                              | `PASS`                      | Linux all-target release clippy 与 philosophy gate 通过 |
| `git diff --check`                                                                           | `PASS`                      | platform capability change 无 whitespace error          |

提交：capability façade `982cb1642a`；backend matrices `5d77a17d79`；text input bridge
`efd05dd0cb`；input source `7c1e5e0de2`；window host `07b4298de0`；system services
`9c8de23b7c`；app lifecycle `1ff5d99840`；accessibility bridge `42a8e9765d`；renderer
factory `85fffe8fe5`。
下一步：继续拆 `WindowHost`/platform system capability，并覆盖
frame lifecycle、IME、clipboard、window controls 和 `run_embedded`/外部 event loop。

### 阶段 7：UI 集成边界

状态：`IN PROGRESS`

已完成的第一步：

- 删除 `ui_input::ERASED_EDITOR_FACTORY` process-global `OnceLock`。
- 增加 per-App `ErasedEditorFactory` global registration；Editor 在 app 初始化时显式注册，
  InputField、Picker 和 RemoteConnection 通过当前 App 的 adapter 读取。
- `ui_prompt` renderer 不再读取 `WorkspaceSettings` 或订阅 product settings；system prompt
  policy 由 `zzz` app composition 决定，generic renderer 只负责 prompt view。
- 保留 `ErasedEditor` façade 和 Editor UTF-16/IME 行为，未引入跨 crate product API。

验证：

| 命令或检查                                                                  | 结果                      | 证据                                                                                                                                                              |
| --------------------------------------------------------------------------- | ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cargo check --locked -p ui_input -p editor -p picker -p remote_connection` | `PASS`                    | app-scoped factory consumers 编译通过                                                                                                                             |
| `cargo check --locked -p ui_prompt -p zzz`                                  | `PASS`                    | prompt policy moved to app composition                                                                                                                            |
| `cargo test --locked -p ui_input --lib`                                     | `PASS`                    | 0 tests, compile/test harness passed                                                                                                                              |
| `./script/clippy -p ui_input`                                               | `PASS`                    | all-target release clippy 与 philosophy gate 通过                                                                                                                 |
| `./script/clippy -p ui_prompt`                                              | `PASS`                    | all-target release clippy 与 philosophy gate 通过                                                                                                                 |
| `cargo test --locked -p editor ime`                                         | `PASS`                    | 6 IME/composition tests passed                                                                                                                                    |
| `cargo test --locked -p editor focus`                                       | `PASS`                    | 2 focus tests passed                                                                                                                                              |
| `cargo test --locked -p editor input`                                       | `PASS`                    | 8 UTF-16/multi-cursor input tests passed                                                                                                                          |
| `git diff --check`                                                          | `PASS`                    | ui_input boundary change 无 whitespace error                                                                                                                      |
| `cargo test --locked -p agent_ui --lib`                                     | `FAIL (baseline overlap)` | 293 passed；6 agent action/focus failures remain; representative baseline reproduction is recorded in `.tmp/gpui-refactor/phase-9/baseline-agent-ui-form-tab.log` |

提交：app-scoped editor adapter `f4f22a68fc`；prompt policy/renderer split `d90153f6cc`。
下一步：补齐显式 prompt renderer adapter 和 UI integration tests，再运行 EXP-009/010。

### 阶段 8：invalidation 实验

状态：`NOT RUN`

EXP-001/002 的 production-like Editor workload 和正式 phase budget 尚未完成，因此
没有引入 scoped invalidation API，也没有用未达标数据声称 20% phase-work 下降。
该阶段保持待运行，后续若 gate 不达标将记录拒绝并永久保留完整 `cx.notify()` 语义。

### 阶段 9：收敛与最终验证

状态：`NOT STARTED`

最新 workspace validation：`cargo test --workspace --locked --no-fail-fast` 完成 workspace
编译，但测试阶段包含已在计划 baseline 复现的 agent_ui action/focus failures，并有多个
editor formatter/inlay tests 长时间运行；为避免无界 session 已中断。精确记录见
`.tmp/gpui-refactor/phase-9/workspace-test-summary.txt`。最终阶段仍需在收敛后重新运行
并区分 baseline failure、environment hang 和真实回归。

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
