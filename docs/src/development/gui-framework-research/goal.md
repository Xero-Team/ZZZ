---
title: GPUI Refactoring Goal
description: Copyable Goal mode instruction for executing the complete GPUI refactor.
---

# GPUI 完整重构 Goal

在仓库根目录开启一个新对话，把下面整段复制到 composer。Codex 官方
[Prompting 指南](https://developers.openai.com/codex/prompting#goal-mode)建议先用
`/plan` 明确方案，再用
[`/goal`](https://developers.openai.com/codex/reference/slash-commands)启动持续执行；
本仓库已经完成 plan，所以可以直接设置 Goal。

```text
/goal
在 /home/begonia/Documents/Github/Xero-Team/ZZZ 中完成 GPUI 基础设施的完整重构。

先阅读并遵守根 AGENTS.md、.rules、docs/AGENTS.md，以及：
- docs/src/development/gui-framework-research/report.md
- docs/src/development/gui-framework-research/refactoring-plan.md
- docs/src/development/gui-framework-research/experiments.tsv
- .agents/skills/absorbing-upstream/SKILL.md 与 REFERENCE.md

把 refactoring-plan.md 作为执行合同，按阶段 0 到阶段 9 持续工作，不要只做分析、
原型或第一阶段后停止。若当前工作区未隔离，在代码修改前创建或切换到专用分支
refactor/gpui-architecture；保留用户已有修改，不得 reset、clean、覆盖或丢弃。
不要 push、开 PR 或合并 main。

开始时复核当前 HEAD 与计划基线；若 HEAD 已前进，先检查覆盖 crate 的差异并在进度
账本中更新 baseline。创建并持续维护
docs/src/development/gpui-refactor-progress.md，记录每阶段的改动、实验、验证命令、
PASS/FAIL/BLOCKED/NOT RUN、提交 SHA、上游来源和下一步。原始 benchmark、pixel
output 与临时数据放在 .tmp/gpui-refactor/。

参考仓库位于 .tmp/ui_ref/，保持只读。任何 Zed 代码都必须按 absorbing-upstream
流程逐 commit 做 A/B/C 分类；不增加 named upstream remote，不恢复 ZZZ 已删除的
账号、遥测、协作、原生 Agent 或其它商业表面。AccessKit 需要分别审查 core
semantics、action routing 和平台 adapter，不得把缺失的 writer 路径整块带入。

按计划完成 frame diagnostics、ThreadedDispatcher 与 accessibility 验证边界、真实
headless renderer、Window 内部 owner 拆分、不可变 BuiltFrame、render contract、
WGPU 模块化、platform capabilities、TextInputClient 与 ui_* 边界清理。只有
EXP-004/005 达标才创建 gpui_render crate；只有 EXP-007 达到至少 20% phase work
下降且无 pixel/input/focus/IME/a11y 差异，才保留 scoped invalidation。实验不达标时
记录拒绝结论并回退该可选设计，然后继续其余必做阶段。

保持 gpui 公开 façade、Entity/Context 所有权语义、request_layout→prepaint→paint、
cached replay、Task drop-cancellation、focus/key/pointer dispatch 和 Editor UTF-16/
multi-cursor IME 行为兼容。每个提交只做一个可回退的逻辑改动，使用 git commit -s；
不得留下不编译的中间提交、未说明 TODO、双实现或无删除条件的兼容层。

每次改动运行最窄相关 check/test；每阶段运行计划中的实验和验证。最终必须运行并
记录 cargo fmt --all -- --check、cargo test --workspace、./script/clippy、
./script/check-philosophy、docs Prettier check，以及所有当前主机可执行的 Linux/
headless/manual smoke。macOS、Windows 和 native screen-reader runtime 无法在当前
主机执行时，写出精确 runbook 并标记 NOT RUN，不得声称通过。

持续自主推进，遇到普通编译错误、测试回归、冲突或设计取舍时自行调查、修复或按
计划回退。只有缺少必须由我提供的产品决策、凭证、硬件或外部平台结果时才提问。
达到 refactoring-plan.md 的完整完成定义、更新最终架构与完成报告、工作树中没有
未解释的重构残留后，才把 Goal 标记 complete。
```
