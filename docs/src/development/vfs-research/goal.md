---
title: VFS Refactoring Goal
description: Copyable Goal command for executing the complete ZZZ VFS refactor.
---

# VFS 完整重构 Goal

截至 2026-10-05，OpenAI 官方
[Developer commands](https://learn.chatgpt.com/docs/developer-commands?surface=ide#set-or-view-a-task-goal-with-goal)
说明 `/goal <objective>` 会把长期目标附加到当前对话。Objective 最长 4,000
字符；更长的执行约束应写入文件并由 Goal 引用。因此下面的命令只定义目标和文档
入口，完整合同和执行状态在受版本控制的报告、计划、账本与实验表中。

在仓库根目录开启新对话，把下面整段作为一条消息发送：

```text
/goal 在当前 ZZZ 仓库根目录完整实施 IDE 级 VFS 重构，直到 docs/src/development/vfs-research/refactoring-plan.md 的“目标与完成定义”全部满足后才标记 Goal complete。

开始前完整阅读并遵守 AGENTS.md、.rules、docs/AGENTS.md，以及：
- docs/src/development/vfs-research/report.md
- docs/src/development/vfs-research/refactoring-plan.md
- docs/src/development/vfs-research/sources.tsv
- docs/src/development/vfs-research/evidence.tsv
- docs/src/development/vfs-research/experiments.tsv
- docs/src/development/vfs-refactor-progress.md

把 refactoring-plan.md 作为执行合同，按 Phase 0 到 Phase 10 连续实施。不要只做分析、计划、原型、ZIP 查看器或第一阶段后停止。每次恢复先读进度账本，从 Next action 继续，不重做已验收阶段。以正确、无损、跨平台、可扩展和干净代码为最高优先级；允许大面积重构和长工期，不以最小 diff、短期兼容或快速交付牺牲架构正确性。

保留用户已有修改，不得 reset、clean、覆盖或丢弃。代码修改前使用专用分支或隔离 worktree；不要 push、开 PR 或合并 main。持续维护 vfs-refactor-progress.md，记录 baseline、设计决策、改动、实验、完整验证命令与 exit status、PASS/FAIL/REJECTED/BLOCKED/NOT RUN、提交 SHA、compatibility adapter 清单和下一步。每个可独立回退的阶段性改动使用 git commit -s；不得留下不编译提交。

严格实现报告中的 provider + snapshot 架构、无损 binary path wire format、ResourceId/VfsPath、positioned I/O、LocalProvider、RemoteProviderProxy、watch sequence/overflow/rescan、断线恢复、expected-version 写入、consumer 迁移、LspPathMapper、native execution boundary 和 data-local ArchiveProvider。不得把 std::PathBuf、URI string、to_string_lossy、OpenDAL、rust-vfs 或 Wasmer virtual-fs 变成核心身份或公开 VFS；第三方库只能按报告规定用于 provider 内部。

遵守 Rust/GPUI/i18n 规范：不新增 unwrap，不用 let _ = 丢弃错误，不创建 mod.rs，使用完整变量名；Entity update 使用内部 cx，所有 spawn Task 必须 await、detach 或存储；用户可见文本同步 en.json 与 zh-CN.json。每阶段运行 targeted check/test 和 ./script/clippy -p ...，阶段结束审查 code smells；最终运行 cargo fmt --all -- --check、cargo test --workspace、./script/clippy、./script/check-philosophy、proto 检查、mdBook build 和 docs Prettier。当前主机不能执行的平台验证必须写可执行 runbook 并标记 NOT RUN，不能冒充 PASS。

安全范围内自主推进，不因工作量大或阶段完成而暂停。只有缺少会改变产品语义、协议兼容或外部 authority 的用户决定时才询问。实验失败时按计划记录并拒绝可选设计，然后继续其余必做阶段。只有全部完成定义、实验账本、清理、文档和最终验证完成后，才把 Goal 标记 complete。
```

查看状态时发送 `/goal`。官方文档还支持 `/goal edit`、`/goal pause`、
`/goal resume` 和 `/goal clear`。
