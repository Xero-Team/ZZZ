---
title: Database Explorer Goals
description: Frozen product and architecture decisions for the ZZZ Database Explorer implementation.
---

# 数据库浏览器冻结决策

本文件是[实现计划](./implementation-plan.md)的冻结决策集。执行期不得单方面推翻；确需变更时
先修改本文件并写明理由，再改实现。面向用户的说明不写在这里，留到 `docs/src/database.md`
（见 D-14，功能可用后创建）。

基线：ZZZ `main` @ `3af1429a83`。决策状态：`FROZEN`。

## D-01 范围：只做关系型 {#d-01}

数据源抽象按 `catalog` → `schema` → `table` / `view` → `column` →
主键 / 索引 / 外键建模。第一版不纳入 MongoDB、Redis 等 NoSQL。理由：用户列出的全部
刚需（schema 树、单元格编辑、事务、Visual EXPLAIN、ER 图）都是有固定 schema 的关系型
语义；引入无 schema 数据源会迫使结果集与 schema 树抽象泛化，牺牲正确性。

## D-02 驱动：SQLite 内建，其余走扩展 {#d-02}

`DatabaseDriver` trait 定义在 `crates/database_core`，SQLite 实现内建。PostgreSQL、
MySQL、DuckDB、ClickHouse 等由扩展提供，主仓库不引入重量级驱动依赖。理由：保持
`cargo build` 时间与发布体积可控，符合 [Keeping ZZZ Lean](../../development/bloat.md)
与 `script/check-philosophy` 的本地优先约束。

内建 SQLite 复用 workspace 已 bundled 的 `libsqlite3-sys`（`Cargo.toml:618`）与
`crates/sqlez` 的连接封装，不新增第三方 SQLite crate。`crates/db` 是应用自身状态库
（`AppDatabase` / `AppMigrator`），不得复用其连接，也不得改动它。

## D-03 MCP 只做读取，不做数据库驱动 {#d-03}

ZZZ 的 `context_servers` 是 MCP 客户端（`docs/src/ai/mcp.md`），不是驱动加载机制。
数据库驱动通过扩展 API 注册；MCP 仅用于把只读查询能力暴露给外部 AI 代理（Phase 12），
以及可选地从已有 MCP server 消费数据，二者不得混为一层。理由：把网络协议当成驱动层会
把连接生命周期、事务与凭据管理绑到不可控的外部进程上。

## D-04 扩展机制：command + JSON/stdin/stdout {#d-04}

扩展注册一个 `database_drivers` 清单条目，指向扩展自带的命令；主机用
`smol::process::Command` 启动该命令，通过 stdin/stdout 以换行分隔 JSON 交换请求与响应。
不在 WIT 里塞复杂资源类型。理由：WIT 接口（`crates/extension_api/wit/`）是稳定的
WASM ABI，加结构化数据库资源会污染所有扩展版本；命令协议可独立演进，且能被任意语言
编写的扩展实现。口令等凭据由主机从 `credentials_provider` 取出后注入子进程环境，不写
入扩展可见的配置文件。

## D-05 写操作：显式事务 + 二次确认 {#d-05}

单元格编辑先暂存为 pending edit，只有 `Commit` 才在事务内生成参数化 DML 提交；
`Rollback` 丢弃。事务跨越连接生命周期，关闭连接前若有未提交变更必须提示。以下语句
在提交前必须二次确认：无 `WHERE` 的 `UPDATE` / `DELETE`、`DROP` / `TRUNCATE` / `ALTER`
等 DDL。理由：数据库写操作不可逆，用户明确选择保守模型；对齐安全约束中"不可逆操作
确认"的要求。

## D-06 结果集与 CSV 预览的关系 {#d-06}

`crates/csv_preview` 的 `TableDataEngine`、`TableLikeContent` 与渲染层是**只读分隔文本**
预览，绑定 `Editor` buffer。结果集是独立类型 `ResultSet`（列元数据 + 行 + 状态 +
分页/流式游标 + 可编辑性），放在 `crates/database_core`。允许把两者共享的表格交互
（列宽拖拽、虚拟列表、复制、右键菜单）抽取为 `crates/ui` 的公共能力并重构
`csv_preview` 复用，但**不允许**把结果集塞进 `TableLikeContent`，也不允许让
`csv_preview` 依赖 `database_core`。理由：结果集有状态机、游标、类型系统与写回语义，
与静态文本表格是不同的领域对象。

## D-07 单元格值类型 {#d-07}

`Value` 至少覆盖 `Null` / `Integer` / `Real` / `Text` / `Blob` / `Date` / `DateTime` /
`Time` / `Bool` / `Numeric(decimal)` / `Json`，保留列的声明类型 `ColumnType`。
BLOB 默认显示为十六进制摘要与长度，不内联渲染二进制。数值与文本保留类型而非全部转
字符串，否则排序、编辑回写与导出会失真。

## D-08 取消语义 {#d-08}

`QueryHandle` 带 `cancel()`；SQLite 走 `sqlite3_interrupt`，扩展驱动向子进程发
`cancel` 请求并终止该查询（不一定是终止进程）。取消后结果集状态为 `Cancelled`，
不显示为错误。所有查询任务用 `cx.spawn` / `cx.background_spawn` 并 await、detach
或存字段，不得丢弃。

## D-09 凭据与连接配置 {#d-09}

连接配置存用户 `settings.json`（`database.connections`，仅非敏感字段），口令与令牌
存 `credentials_provider`（系统 keychain）。连接状态是运行时状态，不得持久化。
默认设置不得含任何远程数据库主机，避免触发 `script/check-philosophy`。理由：凭据
不得落明文文件；状态与配置分离避免重放过期连接。

## D-10 远程与 SSH {#d-10}

远程项目中的 SQLite 文件走 `crates/fs` 与 `crates/remote_connection`，不假设本地路径。
SSH 隧道复用 `crates/repl/src/kernels/ssh_kernel.rs` 与 `crates/remote_connection` 的
既有机制，不重写隧道实现。理由：避免第二套远程路径与连接池语义。

## D-11 查询历史 {#d-11}

查询历史是本地 `crates/db` 域（新 migration + typed query），记录 SQL 文本、连接标识、
时间与行数，不记录结果集内容。理由：与应用自身持久化保持一致，不引入第二个数据库。

## D-12 不做的形态 {#d-12}

不做 Web / 容器自托管形态、团队角色权限、账号体系、默认云端主机。理由：
`script/check-philosophy` 与 `docs/src/migrate/intellij.md:267` 已确立本地优先、无账号、
无遥测；GPUI 是原生渲染，没有可复用的 web 端。对标对象只取 DBeaver / Beekeeper /
DbGate / Tabularis / DB4S / pgAdmin / HeidiSQL 的桌面交互语义。

## D-13 正确性与重构许可 {#d-13}

正确性、可维护性与架构优雅优先于最小 diff。允许大面积重构（含 `csv_preview`、
`extension_api`、`ui` 的共享表格能力抽取），但不得留下双实现、死兼容路径、无 owner
后台任务、未处理的 fallible 操作或未使用的 `TODO`。每个提交只含一个可独立回退的变化，
使用 `git commit -s`，阶段结束前不留不编译提交。

## D-14 文档边界 {#d-14}

`docs/AGENTS.md` 规定不得为未发布功能写推测性用户文档。因此 Phase 0–11 只在
`docs/src/development/database/` 写开发文档与账本；`docs/src/database.md` 用户文档与
`SUMMARY.md` 的用户侧条目留到功能可用后（Phase 13）补，且只写已实现的行为。
