---
title: VFS Convergence Goal
description: Copyable Goal command for executing the VFS convergence and extension plan.
---

# VFS 收敛与拓展 Goal

OpenAI 官方
[Developer commands](https://learn.chatgpt.com/docs/developer-commands?surface=ide#set-or-view-a-task-goal-with-goal)
中的 `/goal <objective>` 会把长期目标附加到当前对话，Objective 最长 4,000 字符。
因此下面的命令只定义目标、入口和不可妥协的约束；完整的阶段合同、决策与执行状态在
受版本控制的文档里。

前置条件：

1. 已按[实现计划](./implementation-plan.md#phase-0)把本目录文档提交到 `main`，并创建
   隔离 worktree `../ZZZ-vfs-convergence` 与分支 `vfs-convergence`（worktree 只检出
   已提交内容，文档未提交时新工作树里没有计划）。
2. 在该 worktree 根目录开启新对话，把下面整段作为一条消息发送。
3. `/goal` 是会话级目标；跨会话恢复时先发送 `/goal` 查看状态，再继续。

```text
/goal 在 ../ZZZ-vfs-convergence 的 vfs-convergence 分支上，按 docs/src/development/vfs-convergence/implementation-plan.md 逐条实现 Phase 0 到 Phase 16，直到其“完成定义”18 条全部满足才标记 complete。

开始前完整阅读并遵守 AGENTS.md、.rules、docs/AGENTS.md，以及 docs/src/development/vfs-convergence/implementation-plan.md、decisions.md、progress.md，并参考 docs/src/development/vfs-research/report.md、adr.md、refactoring-plan.md 与 docs/src/development/vfs-refactor-progress.md 理解历史。注意区分两套编号：旧计划（vfs-research/refactoring-plan.md）的 Phase 0 到 Phase 9 已经合并进 main，不要重做；本计划（vfs-convergence）自己的 Phase 0 到 Phase 16 全部要执行，其中 Phase 0 是账本校准与 baseline，不能跳过。

把 implementation-plan.md 当作执行合同：按阶段顺序推进，每个阶段先读 progress.md 的 Next action，从上次中断处继续。每完成一个工作项立即更新 progress.md，记录 baseline、改动、实验 ID、验证命令与 exit status、PASS/FAIL/REJECTED/NOT RUN、提交 SHA 和下一步。decisions.md 是冻结决策，执行期不得单方面推翻；确实需要变更时先改 decisions.md 并说明理由，再改实现。

实验必须先于修复：每个性能或语义改动前先跑对应 CONV-EXP 基线，改完再跑一次并记录。实验 FAIL 时按计划记录并拒绝对应可选设计，但不得因此跳过硬性完成定义。不得只做分析、计划或第一阶段后停止。

正确性、可维护性与架构优雅优先于最小 diff 和短期兼容；允许大面积重构，但不得留下双实现、死兼容路径、无 owner 后台任务或未处理 fallible 操作。每个提交只含一个可独立回退的变化，使用 git commit -s，阶段结束前不留不编译提交。每个阶段结束用 hunting-code-smells 审查 touched diff。

遵守代码规范：不新增 unwrap() 或可能 panic 的索引；不用 let _ = 丢弃 fallible 结果；不创建 mod.rs；使用完整变量名；Entity::update 与 update_in 使用内部 cx 且不重入；所有 cx.spawn 与 cx.background_spawn 必须 await、detach 或存储；GPUI 测试使用 executor timer 而非 smol::Timer::after；用户可见文本走 i18n，en.json 与 zh-CN.json key 集合一致且 in-code fallback 与 en.json 完全相同。

遵守计划第 4.3 节的架构不变量，特别是：path identity 与 display string 分离；consumer 不按 local/remote/archive 选择实现；capability 决定能否、authorizer 决定是否允许；watcher 是提示、snapshot 是真相；unmount 后身份必须失效；native path 永不作持久化身份；同一事实只有一个 owner。

保留用户已有修改，不得 reset、clean、checkout 覆盖或丢弃。不 push、不开 PR。全部完成定义满足后，先请用户确认，再把 vfs-convergence 合并回 main（默认 --no-ff 保留阶段边界）。

验证：每个阶段跑受影响的 cargo check / cargo test 与 ./script/clippy -p 目标 crate；阶段 16 运行 cargo fmt --all -- --check、cargo test --workspace、./script/clippy、./script/check-philosophy、./script/check-todos、cd docs && npx prettier --check src/ 和 mdbook build docs。本机无法执行的平台或工具验证必须写可执行 runbook 并标记 NOT RUN，不得冒充 PASS。

安全范围内自主推进，不因工作量大或阶段完成而暂停。只有缺少会改变产品语义、协议兼容或外部 authority 的决定时才询问用户；其余自行决策并把理由写进 progress.md。
```

查看或控制状态时发送 `/goal`、`/goal edit`、`/goal pause`、`/goal resume`、
`/goal clear`。

## 使用注意 {#notes}

- 目标文本约 2,100 字符，远低于 4,000 上限；不要在命令里追加计划正文，否则会超出
  并被截断。
- 如果在同一会话里已有旧的 VFS Goal，先用 `/goal clear`，避免两个目标并存。
- 该命令不包含分支创建；worktree 未准备好时，代理会先停在阶段 0 的准备工作，这是
  预期行为。
