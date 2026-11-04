---
title: Database Explorer Goal
description: Copyable plain-text prompt for a CLI agent to execute the ZZZ Database Explorer implementation plan.
---

# 数据库浏览器 Goal 提示词

下面是给 CLI 代理执行的纯文本提示词。它不依赖任何 CLI 特有语法，Codex、Claude Code、
OpenCode 或其他能读仓库文件的 agent 都可直接使用。提示词只定义目标、入口与不可妥协的
约束；完整的阶段合同、决策与执行状态在受版本控制的文档里。

复制时请连同代码块内全部文字一起发送。不要附加计划正文，计划由代理从文档读取。

```text
你在 ZZZ 仓库（Rust workspace，Zed fork，原生 GPUI 编辑器）中实现「数据库浏览器」。
当前仓库没有任何数据库查看/管理功能：crates/db 与 crates/sqlez 只服务应用自身状态，
docs/src/languages/sql.md 只有高亮与格式化，crates/csv_preview 是只读分隔文本预览。
你的任务是从 0 到 1 建好完整能力，不是补全某个已有半成品。

执行入口：完整阅读并遵守仓库根目录 AGENTS.md、.rules、clippy.toml、docs/AGENTS.md，
以及以下文档（按顺序）：
1. docs/src/development/database/decisions.md —— 冻结决策 D-01 至 D-14
2. docs/src/development/database/implementation-plan.md —— 执行合同（目标、完成定义、
   架构、crate 划分、阶段计划、代码规范、风险）
3. docs/src/development/database/progress.md —— 执行账本，每次开始或恢复时先读它，
   从 Next action 继续
参考 docs/src/development/feature-process.md 与
docs/src/development/vfs-convergence/implementation-plan.md 理解本仓库的方法论与文档风格。
实现前还要读 implementation-plan.md 第 6 节列出的全部现成件源码（csv_preview、
sqlez、db、credentials_provider、ui data_table、extension_api、workspace Item/Panel、
main.rs 的 init 注册等），优先复用而非重写。

按 implementation-plan.md 的 Phase 0 到 Phase 13 顺序推进，直到其「完成定义」12 条
全部满足才标记 complete。每个阶段先读 progress.md 的 Next action，从上次中断处继续。
每完成一个工作项立即更新 progress.md，记录 baseline、改动、实验与验证命令和 exit
status、PASS/FAIL/REJECTED/NOT RUN、提交 SHA 和下一步。decisions.md 是冻结决策，执行期
不得单方面推翻；确实需要变更时先改 decisions.md 说明理由，再改实现。

范围：关系型数据库（catalog/schema/table/view/column、主键/索引/外键）。第一版内建
SQLite 驱动并打通：连接管理与测试连接、凭据走 credentials_provider（口令绝不落明文
文件或日志）、schema 树、从 SQL 编辑器执行当前语句或选中语句、长查询可取消且取消
不是错误、结果集虚拟滚动/列宽/排序/复制/导出 CSV、单元格编辑与显式事务提交和回滚、
无 WHERE 的 UPDATE/DELETE 与 DDL 二次确认、ER 图、Visual EXPLAIN、查询历史、
Notebook 式 SQL、MCP 只读暴露。其余方言（PostgreSQL/MySQL/DuckDB 等）通过扩展驱动
协议接入，主仓库不引入重量级驱动依赖。不做 Web/容器自托管、团队权限、账号体系、
NoSQL、默认远程主机。

架构红线（对应 decisions.md 与计划第 7 节不变量）：新增 crates/database_core
（协议与模型）与 crates/database_ui（UI），依赖严格单向，database_core 不依赖
database_ui，csv_preview 不依赖 database_core；结果集是独立类型 ResultSet，不得塞进
csv_preview 的 TableLikeContent；能力声明决定 UI 动作可用性，禁止按 driver id 分支；
凭据单一来源、状态与配置分离、取消是一等公民、同一事实只有一个 owner、路径不假设
本地、错误分类化且 i18n 文案不按错误文本匹配、后台任务与子进程与事务都有 owner
和关闭路径。扩展驱动用 command + JSON over stdin/stdout，不污染 WIT ABI；MCP 只做
只读暴露，不当数据库驱动层。

正确性与可维护性优先于最小 diff 和短期兼容；允许大面积重构（含抽取 csv_preview
的公共表格交互、扩展 extension_api 与 ui），但不得留下双实现、死兼容路径、无 owner
后台任务、未处理 fallible 操作或未兑现的 TODO。每个提交只含一个可独立回退的变化，
使用 git commit -s，阶段结束前不留不编译提交。每个阶段结束用 hunting-code-smells
审查 touched diff。保留用户已有修改，不得 reset、clean、checkout 覆盖或丢弃；不 push、
不开 PR。合并回 main 之前先请用户确认。

代码规范：不新增 unwrap()、可能 panic 的索引、let _ = 丢弃 fallible 结果；不创建
mod.rs；用完整变量名；新 crate 用 [lib] path = "src/xxx.rs"；Entity::update 与
update_in 使用内部 cx 且不重入；所有 cx.spawn 与 cx.background_spawn 必须 await、
detach 或存储；GPUI 测试用 executor timer 而非 smol::Timer::after；外部进程用
smol::process::Command；用户可见文本走 i18n，en.json 与 zh-CN.json 的 key 集合一致
且 in-code fallback 与 en.json 完全相同；注释只写「为什么」。

验证：按范围由窄到宽跑 cargo check -p database_core -p database_ui、对应 cargo test、
cargo xtask clippy（不是 cargo clippy）、cargo test --workspace、cargo xtask check、
cargo fmt --all -- --check、cd docs && npx prettier --check src/ 与 mdbook build docs。
本机无法执行的平台验证写可执行 runbook 并标记 NOT RUN，不得冒充 PASS。用户侧文档
docs/src/database.md 与 SUMMARY.md 条目留到功能可用后（Phase 13）才写，且只写已实现
行为；开发期文档写在 docs/src/development/database/ 下。

安全范围内自主推进，不因工作量大或单个阶段完成而暂停。只有缺少会改变产品语义、
协议兼容或需要外部 authority 的决定时才询问用户；其余自行决策并把理由写进
progress.md。开始工作前先确认隔离 worktree/分支与 baseline（参照计划 Phase 0），
在 main 上先提交本目录文档再建 worktree，否则新工作树里没有计划。
```

## 使用注意 {#notes}

- 提示词约 2,900 字符，是普通自然语言，不含 `/goal` 之类 CLI 专有命令；若你的 CLI
  有会话级 goal 机制，可自行用其语法包装这段文字。
- 计划正文、冻结决策与账本都在受版本控制的文档里，代理应主动读取，不要把整份计划
  粘进提示词。
- 若在同一会话里已有其他长期目标，先清除或明确优先级，避免两个目标互相干扰。
- 提示词不含分支创建步骤；worktree 未准备好时，代理应先停在 Phase 0 的 bootstrap，
  这是预期行为。
