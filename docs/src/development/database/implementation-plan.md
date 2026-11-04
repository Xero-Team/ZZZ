---
title: Database Explorer Implementation Plan
description: Executable contract for building ZZZ's relational database explorer, from SQLite browsing through editing, transactions, and AI integration.
---

# 数据库浏览器实现计划

本文把"为 ZZZ 内建数据库查看/管理能力"转换成可逐条执行、可验收、可恢复的合同。
冻结决策见 [decisions.md](./decisions.md)，执行账本见
[progress.md](./progress.md)，可复制提示词见 [goal.md](./goal.md)。

- 计划基线：ZZZ `main` @ `3af1429a83`
- 前置研究：[Feature Development Process](../feature-process.md)、
  [SQL 语言文档](../../languages/sql.md)、[MCP](../../ai/mcp.md)、
  [IntelliJ 迁移说明](../../migrate/intellij.md)（第 263 行承认当前没有数据库客户端）
- 方法论参照：[VFS 收敛实现计划](../vfs-convergence/implementation-plan.md)
  与 [Telegram Panel Hardening Plan](../telegram-panel-hardening.md)

如果开始执行时 HEAD 已变化，先检查 `crates/csv_preview`、`crates/sqlez`、
`crates/db`、`crates/extension_api`、`crates/extension`、`crates/ui`、`crates/zzz`
的新提交，记录新 baseline 后继续。不得 reset、clean、checkout 覆盖或丢弃现有修改。

## 1. 目标 {#goal}

一句话目标：在 ZZZ 内建成一个原生、本地优先、关系型的数据库浏览器，覆盖从连接管理、
schema 浏览、SQL 执行、结果集查看，到单元格编辑与事务提交、ER 图、Visual EXPLAIN、
查询历史、Notebook 式 SQL 与 AI 集成的完整链路；第一版内建 SQLite 驱动，其余方言通过
扩展协议接入。

ZZZ 当前**没有**数据库客户端。仓库里与"数据库"相关的能力全是旁支：
`crates/db` 与 `crates/sqlez` 只服务应用自身状态，`docs/src/languages/sql.md` 只有
高亮与外部格式化，`crates/csv_preview` 是只读分隔文本预览，
`crates/repl/src/outputs/table.rs` 是 Jupyter 表格输出。这些是**可复用的零件**，不是
已有功能，计划不得把它们误当成半成品来"补全"。

## 2. 竞品对照与取舍 {#prior-art}

按用户给出的三个维度（界面形态、资源占用、支持数据库类型）对照，确定 ZZZ 该学什么、
不学什么。以下特性描述来自用户提供 material，未联网核实，不作为事实背书。

| 参考工具              | 可借鉴的交互语义                                       | 明确不照搬的部分                                                | 对应阶段       |
| --------------------- | ------------------------------------------------------ | --------------------------------------------------------------- | -------------- |
| DBeaver Community     | schema 树层级、ER 图生成、连接属性对话框、导入导出向导 | Java/Eclipse 体积、驱动自动下载与商用 catalog、全谱系数据库广度 | P2, P7         |
| Beekeeper Studio      | 克制的结果集表格、多查询标签页、查询历史保存、SSH 隧道 | Electron 运行时、商用版门禁                                     | P3, P4, P6     |
| DB Browser for SQLite | 双击 `.db` 即开、表结构/单元格浏览、零配置             | 仅限 SQLite 的单文件模型                                        | P1             |
| pgAdmin 4             | 对象属性（索引、触发器、分区、扩展）下钻               | Web 运行时与 PG 专属管理作业                                    | P2, P9         |
| HeidiSQL              | 极轻、响应快、连接切换成本低                           | Windows 原生 / Wine、Delphi 代码                                | 全局非功能约束 |
| DbGate                | SQL + NoSQL 混合、数据比较、可视化图表                 | Web/NAS 自托管形态（见 D-12）、NoSQL（见 D-01）                 | —              |
| Tabularis             | SQL Notebook、Visual EXPLAIN、供 AI 代理调用的 MCP     | 独立 App 形态                                                   | P8, P11, P12   |

**竞争定位**：原生 Rust/GPUI，无 JVM、无 Electron、无 WebView（workspace 依赖里
`Cargo.toml` 不存在这些 crate），因此"资源占用"是既有优势而非缺口。卖点是
"写 SQL → 执行 → 看结果不离开编辑器"，而不是功能广度盖过 DBeaver。

## 3. 完成定义 {#done}

只有同时满足以下条件，整个 Goal 才能标记完成：

1. Phase 0 至 Phase 13 的必做项全部完成；每个条件项有"实施"或"拒绝并附证据"的结论。
2. 能用内建 SQLite 驱动打开本地 `.db` / `.sqlite` 文件，浏览库 → 表/视图 →
   列/类型/主键/索引/外键，且 schema 信息来自实查而非硬编码。
3. SQL 编辑器能执行当前语句或选中语句；结果集支持虚拟滚动、列宽拖拽、排序、复制、
   导出 CSV；长查询可取消，取消后状态是 `Cancelled` 而非错误。
4. 单元格编辑以 pending edit 暂存，显式 `Commit` 才在事务内提交，`Rollback` 丢弃；
   事务未提交时关闭连接会提示；无 `WHERE` 的 `UPDATE`/`DELETE` 与 DDL 有二次确认。
5. 连接配置不含明文口令；口令经 `credentials_provider` 存取；测试连接可用；
   连接状态不被持久化。
6. `DatabaseDriver` 是 trait，且至少有一个非 SQLite 驱动通过扩展协议端到端演示
   （可以是测试用的 mock 驱动，但必须走真实的 subprocess 协议，不是直接函数调用）。
7. schema 能生成 Mermaid `erDiagram` 并在预览中渲染；Visual EXPLAIN 能展示
   SQLite `EXPLAIN QUERY PLAN` 结果；查询历史可检索；Notebook 式 SQL 分单元执行；
   结果集与查询能力通过 MCP 暴露给 agent。
8. `csv_preview` 的公共表格交互若被抽取，重构后其原有测试全部通过，且不存在
   `csv_preview` → `database_core` 的依赖方向。
9. 所有用户可见文本走 `i18n::tr`，`assets/locales/en.json` 与 `zh-CN.json` 的
   key 集合一致，且 in-code fallback 与 `en.json` 值完全相同。
10. 没有新增 `unwrap()`、`let _ =`、`mod.rs`、无 owner 后台任务、未处理 fallible 操作。
    不出现第二套远程路径/连接池实现。
11. `cargo fmt --all -- --check`、`cargo xtask clippy`、`cargo test --workspace`、
    `cargo xtask check`（含 `philosophy` 与 `todos`）、`cd docs && npx prettier --check src/`
    与 `mdbook build docs` 通过，或只剩记录在案并从 clean baseline 复现的既有失败。
12. 执行账本记录每阶段的 baseline、决策、改动、验证命令与 exit status、提交 SHA
    和下一步；每完成一个工作项立即更新 `progress.md`。

## 4. 范围 {#scope}

**做**：关系型数据源、连接管理、schema 树、查询执行与取消、结果集、编辑与事务、
导入导出、ER 图、Visual EXPLAIN、查询历史、Notebook、扩展驱动协议、MCP 只读暴露、
i18n、可访问性、远程项目中的文件访问。

**不做**：Web/容器自托管、团队角色权限、账号体系、NoSQL、驱动自动下载、商用 catalog、
`crates/db` 的改造、默认远程主机（见 D-12）。

## 5. 架构 {#architecture}

### 5.1 Crate 划分 {#crates}

新增两个 crate，不把 UI 与协议混在一起：

```text
crates/database_core/     [lib] path = "src/database_core.rs"
  src/database_core.rs    模块根、pub use、Global 注册
  src/driver.rs           DatabaseDriver / QueryHandle / Connection trait
  src/model.rs            Value, ColumnType, Column, Table, Schema, Catalog,
                          PrimaryKey, Index, ForeignKey, ResultSet, ResultSetState
  src/connection.rs       DataSource, ConnectionConfig, ConnectionStatus,
                          ConnectionManager（含凭据注入、状态机）
  src/session.rs          QuerySession：语句执行、取消、事务、pending edits
  src/schema.rs           schema 树查询与 ER 图 / EXPLAIN 的数据装配
  src/history.rs          查询历史域（依赖 crates/db 的迁移与 typed query）
  src/extension_driver.rs 外部驱动子进程协议（JSON over stdin/stdout）
  src/sqlite/             SQLite 驱动实现（复用 sqlez / libsqlite3-sys）

crates/database_ui/      [lib] path = "src/database_ui.rs"
  src/database_ui.rs      init、Action 注册、Item/Panel 接线
  src/connection_panel.rs 连接列表 Panel（workspace::Panel）
  src/schema_tree.rs      schema 树渲染与下钻
  src/result_set_view.rs 结果集 Item（workspace::Item）、虚拟列表、编辑
  src/query_editor.rs     SQL 编辑器集成、执行 action、Notebook 单元
  src/er_diagram.rs      Mermaid erDiagram 预览接线
  src/explain.rs         Visual EXPLAIN 视图
  src/commit_bar.rs      pending edits / Commit / Rollback / 确认对话框
```

依赖方向（严格单向，禁止环）：

```text
database_ui ──> database_core ──> sqlez, libsqlite3-sys, db, credentials_provider,
                                   fs, remote_connection, extension, ui, gpui, i18n
csv_preview  ──> ui（共享表格交互抽取后，不得反向依赖 database_core）
zzz          ──> database_ui（main.rs 注册 init）
```

`database_core` 不依赖 `database_ui`；`database_ui` 不直接碰 `libsqlite3-sys`。
`crates/zzz/src/main.rs` 参照 `csv_preview::init(cx)`（第 687 行）注册
`database_ui::init(cx)`。新 crate 必须写入根 `Cargo.toml` 的 `members` 与
`[workspace.dependencies]`。

### 5.2 核心契约草案 {#contracts}

以下是语义契约，不是逐字 API 指令。实现时以仓库实际 trait 风格（`crates/fs/src/fs.rs`
用 `async_trait`，`crates/workspace/src/dock.rs:40` 是 `Panel`，
`crates/workspace/src/item.rs:189` 是 `Item`）为准；**语义不得变**，改动签名需在本文件
记录理由。[decisions.md](./decisions.md) 的 D-01 至 D-08 是这些契约的约束来源。

```rust
// driver.rs —— 所有驱动（内建与扩展）实现同一 trait
#[async_trait]
pub trait DatabaseDriver: Send + Sync + 'static {
    fn id(&self) -> &str;
    fn display_name(&self) -> &str;

    async fn connect(&self, config: &ConnectionConfig)
        -> Result<Arc<dyn DriverConnection>>;

    // 能力声明决定 UI 动作可用性，禁止按 driver.id() 分支（对齐 VFS 的 capability 原则）
    fn capabilities(&self) -> DriverCapabilities;
}

#[async_trait]
pub trait DriverConnection: Send + Sync + 'static {
    async fn test(&self) -> Result<()>;
    async fn catalogs(&self) -> Result<Vec<Catalog>>;
    async fn describe_table(&self, table: &TableRef) -> Result<TableSchema>;
    async fn execute(&self, sql: &str, params: &[Value])
        -> Result<(QueryHandle, RowStream)>;
    async fn begin(&self) -> Result<TransactionId>;
    async fn commit(&self, tx: TransactionId) -> Result<()>;
    async fn rollback(&self, tx: TransactionId) -> Result<()>;
    async fn explain(&self, sql: &str) -> Result<ExplainPlan>;
    async fn close(&self) -> Result<()>;
}

pub struct QueryHandle { /* cancel() -> Result<()>; 见 D-08 */ }
impl QueryHandle { pub async fn cancel(&self) -> Result<()>; }
```

`RowStream` 是流式行来源，结果集视图按窗口拉取并虚拟滚动，不一次性物化全部行；
大结果集必须有行数/内存上限并给出可复制的诊断信息（参照
`crates/csv_preview/src/renderer/performance_metrics_overlay.rs` 的可观测性思路）。

```rust
// model.rs —— 结果集是独立领域对象，见 D-06 / D-07
pub enum Value { Null, Integer(i64), Real(f64), Text(String), Blob(Vec<u8>),
                 Date(…), DateTime(…), Time(…), Bool(bool), Numeric(…), Json(…) }
pub struct Column { pub name: String, pub declared_type: ColumnType,
                    pub nullable: bool, pub primary_key: bool }
pub struct ResultSet {
    pub columns: Vec<Column>,
    pub state: ResultSetState,     // Running / Ready / Cancelled / Failed / Closed
    pub editable: bool,            // 由 capabilities 与查询形态共同决定
    /* 窗口化行缓存、游标、排序状态、pending edits 引用 */
}
```

`ConnectionConfig` 只含非敏感字段（driver id、文件路径、host、port、user、database、
显示名）；口令/令牌不进结构体、不进 `settings.json`（D-09）。`ConnectionManager`
持有 `Arc<dyn CredentialsProvider>`，连接前注入凭据，状态机为
`Disconnected → Connecting → Connected → (Querying) → Disconnected`。

### 5.3 扩展驱动协议 {#extension-protocol}

按 D-04：扩展在 `extension.toml` 声明 `[[database_drivers]]`（`id`、`command`、
`dialects`），`crates/extension/src/extension_manifest.rs` 增加对应字段与解析；
`crates/extension_api` 侧只需暴露"取扩展工作目录内可执行文件路径"的既有能力，
**不新增 WIT resource**。主机用 `smol::process::Command` 启动命令，stdin/stdout 以
换行分隔 JSON 交换：

```json
{"id":1,"method":"connect","params":{"config":{...},"credentials":{...}}}
{"id":1,"result":{...}}
```

请求带自增 `id`，响应回带同 `id`；`method` 覆盖 `test` / `catalogs` /
`describe_table` / `execute` / `cancel` / `begin` / `commit` / `rollback` /
`explain` / `close`。错误用 `{"id":1,"error":{"code":"...","message":"..."}}`，
主机映射为 anyhow 错误并经 i18n 呈现分类文案，不按错误文本字符串匹配分支。
子进程生命周期由 `QuerySession` 持有并显式 `close`；不得留下孤儿进程。

## 6. 复用的现成件 {#reuse}

实现前必须读这些文件，优先复用而非重写：

| 现成件            | 位置                                                                                   | 用途                                                  |
| ----------------- | -------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| 表格渲染与列宽    | `crates/ui/src/components/data_table.rs`、`table_row.rs`                               | 结果集表格底座                                        |
| 只读表格交互      | `crates/csv_preview/src/table_data_engine.rs`、`renderer/`                             | 虚拟列表、排序、复制、右键菜单的抽取来源              |
| Jupyter 表格输出  | `crates/repl/src/outputs/table.rs`                                                     | `TabularDataResource` 与 Markdown/HTML 剪贴板导出思路 |
| SQLite 绑定       | `crates/sqlez/src/connection.rs`、`savepoint.rs`、`Cargo.toml:618`                     | 连接、SAVEPOINT（可映射到 pending edit 事务）         |
| 自身状态库        | `crates/db/src/db.rs`、`kvp.rs`、`query.rs`                                            | 查询历史域的迁移与 typed query 范式                   |
| 凭据              | `crates/credentials_provider/src/credentials_provider.rs`                              | keychain 读写删                                       |
| SSH 隧道          | `crates/repl/src/kernels/ssh_kernel.rs`、`crates/remote_connection`                    | 远程连库的隧道与连接池                                |
| Mermaid ER        | `crates/mermaid_render/src/mermaid_render.rs:219`                                      | `erDiagram` 渲染（已有 CJK 测试）                     |
| MCP               | `crates/context_server`、`docs/src/ai/mcp.md`                                          | 只读查询工具暴露                                      |
| Item / Panel 范式 | `crates/csv_preview/src/csv_preview.rs:293`、`project_panel/src/project_panel.rs:8398` | 注册与 dock 接线                                      |
| Quick Action 预览 | `crates/zzz/src/zzz/quick_action_bar/preview.rs`                                       | `.db` 文件打开入口的注册方式                          |

## 7. 架构不变量 {#invariants}

1. **能力决定能否，配置决定是否允许。** UI 动作可用性只查 `DriverCapabilities`，
   不写 `if driver.id() == "sqlite"` 之类的分支。
2. **结果集与静态表格分离。** `ResultSet` 不塞进 `TableLikeContent`；不出现双向依赖。
3. **凭据单一来源。** 口令只经 `CredentialsProvider`，不进配置、日志、错误信息、
   MCP 输出或查询历史。
4. **状态与配置分离。** 连接状态、查询结果、事务不持久化；配置与历史才持久化。
5. **取消是一等公民。** 每个 `execute` 都返回可取消的 `QueryHandle`；取消不报错。
6. **同一事实只有一个 owner。** 连接生命周期归 `ConnectionManager`，事务归
   `QuerySession`，行缓存归 `ResultSet`，不得多处各存一份。
7. **路径不假设本地。** 远程项目里的 SQLite 文件经 `fs` / `remote_connection`，
   不做 `to_str()` 往返或本地绝对路径假设。
8. **错误分类化。** 连接失败、认证失败、语法错误、取消、权限不足有 typed 分类与
   i18n 文案，不按错误文本匹配。
9. **无孤儿。** 后台任务、子进程、事务都有 owner 与关闭路径；丢弃任务即取消的
   GPUI 语义必须被显式 await / detach / 存储处理。
10. **不可逆操作有确认。** 见 D-05，且确认对话框文案必须说明影响行数或对象名。

## 8. 阶段计划 {#phases}

按阶段顺序推进，每阶段先读 `progress.md` 的 `Next action`。每完成一个工作项立即
更新账本（baseline、改动、验证命令与 exit status、PASS/FAIL/REJECTED/NOT RUN、
提交 SHA、下一步）。阶段结束用 `hunting-code-smells` 审查 touched diff。

### Phase 0 — 账本校准与 bootstrap {#phase-0}

1. 在隔离 worktree / 分支上工作（参照 VFS 计划的 worktree 前置步骤）；先在 `main`
   提交本目录文档再建 worktree，否则新工作树里没有计划。
2. 记录 baseline、rustc/OS、`git status`；阅读第 6 节列出的全部现成件。
3. 跑一次 clean baseline 验证：`cargo xtask check`、`cargo test -p csv_preview`、
   `cd docs && npx prettier --check src/`，把既有失败登记为已知项。
4. 产出一页"现成件复核"记录：确认 `Item` / `Panel` / `async_trait` /
   `CredentialsProvider` 的实际签名，并据此定稿第 5.2 节契约。

验收：`progress.md` 有完整 baseline 与已知失败清单；契约草案与本文件一致。

### Phase 1 — 骨架与 SQLite 只读浏览 {#phase-1}

1. 新增 `crates/database_core`、`crates/database_ui`，登记 workspace 与 `main.rs`。
2. 实现 `model.rs`、`driver.rs`、`connection.rs` 的最小版本与内建 SQLite 驱动。
3. 双击 `.db` / `.sqlite` 打开 `ResultSetView`（`Item`）；`ConnectionPanel` 作为
   `Panel` 显示连接列表；schema 树来自实查 `sqlite_master` / `PRAGMA table_info` /
   `PRAGMA foreign_key_list` / `PRAGMA index_list`。
4. i18n 键与两个 locale 同步。

验收：能打开一个真实 SQLite 文件并浏览表、列、主键、索引、外键；`cargo test -p
database_core -p database_ui` 通过。

### Phase 2 — 连接管理与凭据 {#phase-2}

1. `database.connections` 设置 schema（`JsonSchema` + Settings UI 条目）。
2. 新增/编辑/删除连接对话框、测试连接、`CredentialsProvider` 注入。
3. 连接状态机与断开重连；关闭连接时事务提示（与 Phase 5 联动）。
4. `script/check-philosophy` 通过（无默认远程主机）。

验收：口令不出现在任何文件与日志；测试连接对错误口令给出分类错误文案；
默认设置不含远程主机。

### Phase 3 — 查询执行与取消 {#phase-3}

1. SQL 编辑器加 `database: run statement` / `run selection` action 与默认键位
   （检查与既有键位冲突）。
2. 语句切分（选中优先，否则当前语句）；`QuerySession` 执行、`QueryHandle::cancel`、
   `ResultSetState` 状态机；错误分类文案。
3. 多查询标签页/结果集切换。

验收：执行、错误、取消三条路径都有测试；长查询可取消且状态为 `Cancelled`；
无丢弃的 `cx.spawn` 任务。

### Phase 4 — 结果集视图 {#phase-4}

1. 虚拟滚动 + 窗口化行拉取；列宽拖拽、按列排序、单元格复制、导出 CSV /
   Markdown / HTML 剪贴板。
2. 抽取 `csv_preview` 与结果集共享的表格交互到 `crates/ui`（允许重构），
   `csv_preview` 原有测试全绿。
3. BLOB 摘要显示、NULL 与类型化数值的区分渲染；可访问性标签
   （参照 `TableAccessibility`）。

验收：大结果集内存有上限；`csv_preview` 无回归；依赖方向符合第 5.1 节。

### Phase 5 — 编辑、事务与确认 {#phase-5}

1. pending edit 模型（按主键定位行；无主键结果集不可编辑并说明原因）。
2. 参数化 DML 生成、显式 `Commit` / `Rollback`、`commit_bar` 与影响行数展示。
3. 无 `WHERE` 的 `UPDATE`/`DELETE` 与 DDL 的二次确认；事务未提交关闭提示。
4. 用 `crates/sqlez/src/savepoint.rs` 的 SAVEPOINT 语义做事务内回滚验证。

验收：不提交则不落库；回滚后数据不变；确认对话框说明影响对象；
参数化语句杜绝 SQL 注入（有测试）。

### Phase 6 — 扩展驱动协议 {#phase-6}

1. `extension_manifest.rs` 增加 `database_drivers` 解析；实现第 5.3 节子进程协议。
2. 至少提供一个 mock 驱动走真实 subprocess 端到端（非直接函数调用）。
3. 能力声明驱动 UI 可用性；子进程生命周期、错误映射、超时与 `close`。

验收：完成定义第 6 条；扩展进程无孤儿；协议有往返与错误用例测试。

### Phase 7 — ER 图与导入导出 {#phase-7}

1. 从 `TableSchema` 生成 Mermaid `erDiagram`（表、列、主外键），接线
   `mermaid_render` 预览。
2. 结果集与 schema 的 CSV/JSON 导出、表数据导入（事务内）。

验收：生成图对 CJK 表名正确（复用已有 `erDiagram` 测试）；导入在事务内可回滚。

### Phase 8 — Visual EXPLAIN {#phase-8}

1. `EXPLAIN QUERY PLAN` 解析为 `ExplainPlan`，树状/可视化展示。
2. 无 EXPLAIN 能力的驱动按 capability 隐藏该动作。

验收：典型查询的计划可读；能力缺失时不显示动作而非报错。

### Phase 9 — 查询历史 {#phase-9}

1. `crates/db` 新增迁移 + typed query 域，记录 SQL、连接标识、时间、行数。
2. 历史面板检索与重跑；不记录结果集内容与凭据。

验收：重启后历史可查；历史表无口令/BLOB。

### Phase 10 — Notebook 式 SQL {#phase-10}

1. SQL Notebook `Item`：按单元编辑、逐单元执行、结果内嵌、单元可重排。
2. 复用 `crates/repl` 的 notebook 交互语义，但执行走 `QuerySession` 而非 Jupyter
   kernel。

验收：单元执行互不阻塞；结果与单元绑定；取消单个单元不影响其他单元。

### Phase 11 — 远程与 SSH（条件项） {#phase-11}

按 D-10 复用既有隧道。若 clean baseline 下远程环境不可用，写可执行 runbook 并标记
`NOT RUN`，不得冒充 PASS。实施或拒绝都要记录证据。

### Phase 12 — MCP 只读暴露 {#phase-12}

1. 把"列出连接 / 描述表 / 执行只读查询"暴露为 MCP tools（读为主，写需显式开关）。
2. 结果以结构化文本返回，不含凭据与 BLOB；对齐 `docs/src/ai/mcp.md` 的 Tools 能力。

验收：agent 可查询并收到结果；凭据不出现在任何 tool 输出；符合 D-03。

### Phase 13 — 文档、门禁与收尾 {#phase-13}

1. 按 `docs/AGENTS.md` 写 `docs/src/database.md` 用户文档（只写已实现行为），
   在 `SUMMARY.md` 登记用户侧与开发侧条目；跑 Prettier 与 mdbook build。
2. 全量验证：第 3 节第 11 条全部命令；把结果写入 `progress.md`。
3. `hunting-code-smells` 审查全量 diff；逐条核对第 3 节 12 条完成定义与第 7 节
   10 条不变量。

## 9. 代码规范 {#code-style}

遵守 `AGENTS.md`、`.rules`、`clippy.toml`、`docs/AGENTS.md`：

- 不新增 `unwrap()`、可能 panic 的索引、`let _ =` 丢弃 fallible 结果；错误传播到
  UI 层给用户可理解反馈。
- 不创建 `mod.rs`；用完整变量名；新 crate 用 `[lib] path = "src/xxx.rs"`。
- `Entity::update` / `update_in` 用内层 `cx` 且不重入；所有 `cx.spawn` /
  `cx.background_spawn` 必须 await、detach 或存储。
- GPUI 测试用 `cx.background_executor().timer(...)`，不用 `smol::Timer::after`；
  外部进程用 `smol::process::Command`。
- 用户可见文本走 `i18n::tr`（或 crate 别名），`en.json` / `zh-CN.json` key 集合一致，
  fallback 与 `en.json` 完全相同；品牌名、SQL 关键字、内部标识符不翻译。
- 注释只写"为什么"，不写"下一行做什么"。
- 每个提交只含一个可独立回退的变化，用 `git commit -s`；不 push、不开 PR。

验证命令（按范围由窄到宽）：

```sh
cargo check -p database_core -p database_ui
cargo test -p database_core -p database_ui -p csv_preview
cargo xtask clippy
cargo test --workspace
cargo xtask check
cargo fmt --all -- --check
cd docs && npx prettier --check src/ && mdbook build docs
```

注意 `AGENTS.md` 明确 lint 用 `cargo xtask clippy`，**不是** `cargo clippy`。

## 10. 风险与未决 {#risks}

| 风险                                                  | 处置                                                                                                           |
| ----------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| `crates/csv_preview` 表格交互抽取牵连现有渲染与测试   | 先在 Phase 4 抽取最小公共面，保留 `csv_preview` 测试为回归门禁；抽取失败则结果集自建渲染，记录 REJECTED 及原因 |
| SQLite 单连接并发与 `sqlite3_interrupt` 线程模型      | Phase 1 前用实验确认连接是否需串行化；把结论写入 `progress.md`                                                 |
| 扩展子进程协议的跨平台换行/编码/超时                  | Phase 6 在 Linux 验证，macOS/Windows 写 runbook 标记 `NOT RUN`                                                 |
| 远程项目中的 SQLite 文件语义                          | Phase 11 条件项，按证据实施或拒绝                                                                              |
| `docs/AGENTS.md` 禁止为未发布功能写用户文档           | 见 D-14：用户文档留到 Phase 13                                                                                 |
| 上游 Zed 同步可能改动 `extension_api` / `csv_preview` | 每阶段开始前检查相关 crate 的 HEAD 变化并记录 baseline                                                         |

未决项只允许两类：会改变产品语义、或需要外部 authority 的决定。执行中遇到此类
决定才询问用户；其余自行决策并把理由写进 `progress.md` 与必要时
[decisions.md](./decisions.md)。
