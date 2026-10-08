---
title: GPUI Qt Base Gap Completion Goal
description: Copyable persistent Goal mode instruction for implementing the GPUI
  completion plan.
---

# GPUI Qt Base 差距闭环 Goal {#gpui-qtbase-completion-goal}

在仓库根目录的新 Codex 对话中复制下面整段文本。官方 OpenAI 文档说明，`/goal`
用于设置持续目标；对多步骤任务，先用 `/plan` 形成方案，再用 `/goal` 启动持续执行。
本仓库已有经过审查的报告和实施合同，因此可以直接使用这个 Goal。参见
[Codex Prompting 中的 Goal mode](https://developers.openai.com/codex/prompting#goal-mode)
和 [slash commands](https://developers.openai.com/codex/reference/slash-commands)。

```text
/goal
在当前 ZZZ 工作区持续完成 GPUI 的 Qt Base 高价值差距闭环。以正确性、优雅的所有权
与生命周期、跨平台可验证性和长期可维护性为最高优先级。允许必要的跨 crate 重构、
公共内部 API 迁移和长工期；不要为了小 diff、短期兼容、表面功能或快速交付保留错误
架构。

开始前完整阅读并遵守：
- 根 AGENTS.md、.rules、docs/AGENTS.md；
- docs/src/development/gui-framework-research/qtbase-gap-report.md；
- docs/src/development/gui-framework-research/qtbase-completion-plan.md；
- docs/src/development/gui-framework-research/report.md；
- docs/src/development/gui-framework-research/refactoring-plan.md；
- docs/src/development/gpui-refactor-progress.md；
- 任何被修改目录下的 AGENTS.md 与适用 skill。

新计划承接已经完成的 GPUI 基础设施重构，绝不能重新执行或倒退旧计划已验收的阶段
0–9。先审计当前 HEAD、branch 和工作树；保留用户已有改动，禁止 reset --hard、git
checkout --、git clean 或覆盖未知文件。必要时创建专用分支/worktree；不要 push、开
PR、合并 main 或做外部状态变更，除非我明确要求。

把 qtbase-completion-plan.md 作为执行合同，按阶段 0 到阶段 8 持续推进。先建立事实
基线和 support matrix，再做 accessibility correctness、UI component semantics、IME/
accessible text、platform QA 与 API/docs；只有双消费者实验证明价值时才做 model/view
抽象或 public GPU extension。不要把 Qt Core、QML、network、SQL、XML、DBus、打印或
Qt Widgets 逐类复制到 GPUI；不要把 editor buffer、i18n、extension host 或产品状态
塞进 gpui。

创建并持续维护
docs/src/development/gui-framework-research/qtbase-completion-progress.md。每个阶段记录：
baseline、目标、设计决定、改动文件、验证命令及其完整 PASS/FAIL/BLOCKED/NOT RUN
结果、平台/feature 条件、提交 SHA、已知限制和精确 next action。将原始 benchmark、
pixel output、semantic dump 和 runtime logs 放在 .tmp/gpui-qtbase-completion/。

始终保持 Entity/Context ownership、request_layout→prepaint→paint、cached replay、
focus/key/pointer dispatch、Task drop-cancellation、UTF-16/marked-text/multi-cursor IME、
semantic node/action lifecycle 以及 ZZZ 的无账号/无遥测/无默认网络哲学正确。每个
PlatformCapabilities 条目必须由真实后端行为支撑；不得把 feature 编译、mock 或
cross-compile 宣称为 native runtime 支持。Web accessibility 在有真实实现前保持
explicitly unsupported。

对每项变更先写会在旧行为失败的最窄 regression test，再写生产代码。提交保持小而可
回退，使用 git commit -s；不留下无删除条件的 compatibility shim、双实现、dead code
或未说明 TODO。每次修改运行最窄 check/test；每个阶段运行计划定义的全部验证、相关
./script/clippy、script/check-philosophy、git diff --check 和 docs Prettier。最终运行
cargo fmt --all -- --check、cargo test --workspace、./script/clippy、
script/check-philosophy、docs Prettier check，以及当前主机能执行的 headless/Linux/
native runtime fixture。macOS/Windows 无法执行时给出可直接运行的 runbook，并诚实
标记 NOT RUN。

用真实辅助技术验证 VoiceOver、Narrator 或 NVDA、Orca/AT-SPI；semantic snapshot
unit test 不能替代 native proof。组件语义只添加给 interactive、structural、status 和
text-input 内容，避免装饰元素制造可访问噪声。若 model/view prototype 不能同时服务
tree 与 table/list 并减少重复、保持稳定 identity/selection/focus/virtualization/a11y，
删除 prototype 并记录拒绝决定。若 GPU extension 没有两个独立真实消费者，拒绝公开
renderer internals。

持续自主工作。遇到编译错误、测试回归、冲突、设计取舍或可安全调查的平台问题时先
自行查源码、写测试、修复或按计划回退。仅在缺少必须由我决定的产品范围、凭据、设备
或外部平台执行结果时提问。只有 qtbase-completion-plan.md 的完成定义全部满足、最终
架构与文档更新、progress ledger 没有未解释残留后，才将 Goal 标记 complete。
```
