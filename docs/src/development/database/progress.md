---
title: Database Explorer Progress
description: Durable execution ledger for the ZZZ Database Explorer implementation.
---

# 数据库浏览器执行账本

此文件是[实现计划](./implementation-plan.md)的持久执行状态。每次开始或恢复执行时先读
本文件，并从 `Next action` 继续。冻结决策见 [decisions.md](./decisions.md)，可复制提示词
见 [goal.md](./goal.md)。

## 当前状态 {#current-status}

- Goal status: `IN PROGRESS`
- Plan baseline: `main` @ `3af1429a83`
- Current HEAD: `database-explorer` @ `f2a037d404`（文档基线；代码基线 `3af1429a83`
  已 `merge-base --is-ancestor` 验证包含）
- Branch/worktree: `database-explorer` @
  `/home/begonia/Documents/Github/Xero-Team/ZZZ-database-explorer`
- Active phase: `Phase 1`
- Last completed phase: `Phase 0`
- Blocking issue: `None`
- Next action: 执行 Phase 1 工作项 3——`ConnectionManager`（凭据注入、状态机、
  连接生命周期 owner）与 `QuerySession`（执行/取消/事务/pending edits），随后
  `database_ui` 的 Item/Panel 接线、`.db` 打开入口与 `main.rs` 的 `init` 注册。

## Bootstrap {#bootstrap}

Phase 0 已执行：先在 `main` 提交本目录文档集（`f2a037d404`），再建隔离 worktree
`../ZZZ-database-explorer -b database-explorer main`。worktree 只检出已提交内容，
故第 1 步不可省。`git status --porcelain` 为空，代码基线 `3af1429a83` 已包含。

| 项                | 值                                                                | 备注                               |
| ----------------- | ----------------------------------------------------------------- | ---------------------------------- |
| 文档提交 SHA      | `f2a037d404cc396581e24822d34cf2a885b02bc4`                        | `docs: add database explorer plan` |
| Baseline HEAD     | 代码 `3af1429a83`，文档 `f2a037d404`                              | 代码基线仍为 `3af1429a83`          |
| `rustc -Vv`       | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`，`x86_64-unknown-linux-gnu` | LLVM 23.1.1，`cargo 1.99.1`        |
| OS / kernel       | `Linux fedora 7.4.0-0.rc0.261029gc339f4010dea.11.fc46.x86_64`     | `x86_64`                           |
| 初始 `git status` | 空                                                                | 期望为空 ✓                         |

### Clean baseline 验证 {#baseline-validation}

在 `database-explorer` worktree 的 clean checkout 上执行（未含任何实现改动）：

| 命令                                   | Exit status | 既有失败登记                                                        |
| -------------------------------------- | ----------- | ------------------------------------------------------------------- |
| `cargo xtask check`                    | 0           | 6 repository checks passed（含 philosophy、keymaps、todos）         |
| `cargo test -p csv_preview`            | 0           | 16 passed; 0 failed                                                 |
| `cd docs && npx prettier --check src/` | 0           | All matched files use Prettier code style                           |
| `mdbook build docs`                    | 101 → 0     | clean worktree 首次失败，见下方「已知环境前置项」；补生成产物后通过 |
| `cargo fmt --all -- --check`           | 0           | 额外补测（计划第 9 节门禁）                                         |

**已知环境前置项（非代码缺陷，从 clean baseline 复现）：** `crates/docs_preprocessor/actions.json`
被 `.gitignore:53` 排除，是 `script/generate-action-metadata`（内部执行
`cargo run -p zzz -- --dump-all-actions`）生成的产物。clean worktree 没有它，
`docs_preprocessor` 的 `load_all_actions` 返回空 `ActionManifest`（`main.rs:684-695`），
导致 `keymap_schema_for_actions` 产出空 schema、`jsonschema::validator_for` 报
`[] has less than 1 item`，`mdbook build docs` 以 exit 101 失败。处理：从 `main`
复制该产物到 worktree 后，`mdbook build docs` exit 0（201 个 `.html`，仅若干
`No meta description found` warning）。Phase 13 全量门禁前需先跑
`script/generate-action-metadata`（runbook 见该阶段）。此失败与数据库改动无关，
登记为 baseline 固有项，不得冒充 PASS。

## 阶段状态 {#phase-status}

| Phase | Result                   | Status      | Commit                                           | Validation                                     | Notes                               |
| ----- | ------------------------ | ----------- | ------------------------------------------------ | ---------------------------------------------- | ----------------------------------- |
| 0     | 账本校准、worktree、基线 | PASS        | `f2a037d404`（bootstrap 文档提交）+ 本次账本提交 | 见 [baseline-validation](#baseline-validation) | baseline 4+1 门禁与现成件复核均完成 |
| 1     | 骨架与 SQLite 只读浏览   | NOT STARTED | -                                                | -                                              | Next action                         |
| 2     | 连接管理与凭据           | NOT STARTED | -                                                | -                                              | -                                   |
| 3     | 查询执行与取消           | NOT STARTED | -                                                | -                                              | -                                   |
| 4     | 结果集视图               | NOT STARTED | -                                                | -                                              | -                                   |
| 5     | 编辑、事务与确认         | NOT STARTED | -                                                | -                                              | -                                   |
| 6     | 扩展驱动协议             | NOT STARTED | -                                                | -                                              | -                                   |
| 7     | ER 图与导入导出          | NOT STARTED | -                                                | -                                              | -                                   |
| 8     | Visual EXPLAIN           | NOT STARTED | -                                                | -                                              | -                                   |
| 9     | 查询历史                 | NOT STARTED | -                                                | -                                              | -                                   |
| 10    | Notebook 式 SQL          | NOT STARTED | -                                                | -                                              | -                                   |
| 11    | 远程与 SSH（条件项）     | NOT STARTED | -                                                | -                                              | -                                   |
| 12    | MCP 只读暴露             | NOT STARTED | -                                                | -                                              | -                                   |
| 13    | 文档、门禁与收尾         | NOT STARTED | -                                                | -                                              | -                                   |

## 现成件复核 {#reuse-review}

Phase 0 第 4 步完成。读取实现计划第 6 节列出的全部现成件源码，确认实际签名：

| 现成件                       | 实际签名 / 位置                                                                                                                                                                                                                                                                  | 复核结论                                                                                                                                                                                                                                                                               |
| ---------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `workspace::Item`            | `crates/workspace/src/item.rs:189` `pub trait Item: Focusable + EventEmitter<Self::Event> + Render + Sized { type Event; fn tab_content_text(&self, usize, &App) -> SharedString; ... }`                                                                                         | 唯一必需方法 `tab_content_text`，其余有默认实现。`csv_preview.rs:293` 是最小范例（`Event = ()`）。打开走 `pane.add_item(Box::new(view), ...)`。                                                                                                                                        |
| `workspace::Panel`           | `crates/workspace/src/dock.rs:40` `pub trait Panel: Focusable + EventEmitter<PanelEvent> + Render + Sized`                                                                                                                                                                       | 固定 `PanelEvent`（`dock.rs:31`），9 个必需方法（`persistent_name`/`panel_key`/`position`/`position_is_valid`/`set_position`/`default_size`/`icon`/`icon_tooltip`/`toggle_action`/`activation_priority`）。范例 `terminal_mode/src/file_panel.rs:930`。注册走 `Workspace::add_panel`。 |
| `async_trait` 用法           | `crates/fs/src/fs.rs:128` `#[async_trait::async_trait]` 全路径属性                                                                                                                                                                                                               | workspace 依赖已有 `async-trait = "0.1"`（`Cargo.toml:505`）。`DatabaseDriver` / `DriverConnection` 按此风格。                                                                                                                                                                         |
| `CredentialsProvider`        | `crates/credentials_provider/src/credentials_provider.rs:11` `pub trait CredentialsProvider: Send + Sync { fn read_credentials(url, &AsyncApp) -> Future<Option<(String, Vec<u8>)>>; fn write_credentials(url, user, &[u8], &AsyncApp); fn delete_credentials(url, &AsyncApp) }` | 无独立 Global；经 `client.credentials_provider()`（`crates/client/src/client.rs:535`）取 `Arc<dyn CredentialsProvider>`。口令走 keychain，不进配置/日志。                                                                                                                              |
| `DatabaseZap` 图标           | `crates/icons/src/icons.rs:87` `DatabaseZap,`                                                                                                                                                                                                                                    | 已存在，`IconName::DatabaseZap` 可直接用（`Panel::icon` / `Item::tab_icon`）。                                                                                                                                                                                                         |
| `mermaid_render` `erDiagram` | `crates/mermaid_render/src/mermaid_render.rs:179` `pub fn render_to_svg(source: &str, theme: &MermaidTheme) -> Result<String>`；CJK 测试 `:214`                                                                                                                                  | 统一 `render_to_svg`，source 以 `erDiagram\n` 开头；已有 CJK 测试 `er_multibyte_attribute_does_not_crash`。                                                                                                                                                                            |
| `csv_preview::init` 注册     | `crates/zzz/src/main.rs:687` `csv_preview::init(cx);`（在 `*.init(cx)` 序列中）                                                                                                                                                                                                  | 新增 `database_ui::init(cx);` 同处；`csv_preview::init` 用 `cx.observe_new(...).detach()`（`csv_preview.rs:56`）。                                                                                                                                                                     |

补充复核（第 6 节其余现成件）：

- `ui::Table`（`crates/ui/src/components/data_table.rs:487`）已内置 `uniform_list` /
  `variable_row_height_list` 虚拟列表、`ColumnWidthConfig::Resizable` 拖拽列宽、
  `pin_cols`、`TableInteractionState`、`TableAccessibility`。**排序、复制、右键菜单不是
  `Table` 的职责**——`csv_preview` 在 `renderer/table_header.rs`、`table_cell.rs` 自行实现。
  Phase 4 抽取共享交互时以此为界。
- `csv_preview::TableDataEngine`（`table_data_engine.rs:20`）是 `pub(crate)`；
  `TableLikeContent`（`types/table_like_content.rs:7`）是静态文本表格。结果集 `ResultSet`
  必须是独立类型，不塞进去（D-06）。
- `sqlez::Connection`（`connection.rs:12`）是 `unsafe impl Send`、**非 Sync**（含
  `RefCell`），`exec/select` 在 `typed_statements.rs`（`connection.exec(sql)?()?`），
  `with_savepoint` 在 `savepoint.rs:10`。`ThreadSafeConnection`（`thread_safe_connection.rs:34`）
  才 Send+Sync。根 `Cargo.toml:618` `libsqlite3-sys = { version = "0.39.1", features = ["bundled"] }`，
  仓库**无** `rusqlite`。
- `crates/db`：`static_connection!` 宏（`db/src/db.rs:275`）+ `impl Domain` + `query!`
  （`db/src/kvp.rs:20-87` 是完整范式）。`query!` 内用 `(?)` 占位。
- `extension_manifest.rs:82` `ExtensionManifest` 用 `#[serde(default)]` 集合字段；
  命令能力块是 `[[capabilities]]` `kind = "process:exec"` / `command` / `args`。
  WIT 按版本分目录（`since_v0.4.0/extension.wit`），内建 crate 无需动 WIT（D-04）。
- `context_server::McpServerTool`（`crates/context_server/src/listener.rs:414`）：
  `add_tool`（`:95`）、`annotations().read_only_hint = Some(true)` 表达只读。
- `crates/repl/src/outputs/table.rs` 的 `TableView` 用
  `ClipboardItem::new_string_with_html(plain, html)` 预生成 Markdown+HTML 剪贴板。
- locales：顶层 `{"entries": {...}}`，`i18n::tr(cx: &App, key, fallback) -> String`
  （`crates/i18n/src/i18n.rs:126`），key 惯例 `<namespace>.<area>.<item>`，en/zh 各
  3850 键且集合一致。
- `crates/zzz/src/zzz/quick_action_bar/preview.rs:23` `PreviewType` 目前只有
  `Markdown/Svg/Csv/Typst`，无 `.db` 变体；`.db` 打开入口需在业务 crate `init` 里
  注册（`register_action_renderer` + `pane.add_item`）。
- `fs::Fs`（`crates/fs/src/fs.rs:128`）用 `&Path`/`PathBuf`，本地/远程由 `Arc<dyn Fs>`
  抽象，读文件走 `load_bytes`/`open_handle`，不假设本地（D-10）。

### 契约定稿（基于实际签名，语义不变） {#contract-finalization}

计划第 5.2 节是语义契约。按上述实际签名定稿如下，**不改动 decisions.md D-01 至 D-14
的任何语义**：

1. `DatabaseDriver` / `DriverConnection` 用 `#[async_trait::async_trait]`，返回
   `anyhow::Result`。`DriverConnection` 以 `Arc<dyn DriverConnection>` 共享（对齐
   计划 `connect -> Result<Arc<dyn DriverConnection>>`）。
2. `QueryHandle::cancel()` 用**同步** `fn cancel(&self) -> Result<()>` 而非计划草案的
   `async`：取消必须可从 GPUI 事件回调即时触发，SQLite 走 `sqlite3_interrupt`（线程安全，
   `DB-EXP-001` 结论），扩展驱动走向子进程写 `cancel` 帧。语义（取消非错误）不变。
3. `RowStream` 用 `futures::channel::mpsc` 的有界流式行通道 + 行数/内存上限，避免把
   GPUI 类型耦合进 `database_core`；`database_ui` 用 `cx.spawn` 包装并 await/detach/存储。
4. **依赖倒置（实现细化，理由记录于此）**：为让 `database_core` 的核心逻辑（model、
   SQL 分析、EXPLAIN/ER/CSV 装配、协议帧、事务/pending edit 状态机）可用纯 Rust 单元
   测试充分验证，`database_core` 直接依赖 `credentials_provider`（`Arc<dyn
CredentialsProvider>`，满足 D-09 与计划 5.2「ConnectionManager 持有
   `Arc<dyn CredentialsProvider>`」）与 `libsqlite3-sys`（D-02），其余宿主服务
   （`Fs`、`db` 历史存储、扩展子进程、MCP）以最小 trait 抽象，由 `database_ui` 注入真实
   ZZZ 实现。依赖方向仍严格 `database_ui -> database_core`，`csv_preview` 不变。
5. **SQLite 用 `libsqlite3-sys` 直接 FFI**（非 `sqlez` 高层封装）：`sqlez::Connection`
   仅 Send 非 Sync、`open_file` 失败回退内存库、无 `sqlite3_interrupt` 通道与列类型流式
   读取，且 `exec/select` 面向应用自身状态（D-02 明确「不得复用其连接」）。D-02 要求
   复用 bundled `libsqlite3-sys`，满足。此为不引入重量级依赖、可控取消与流式读取所必需。

## 实验记录 {#experiments}

格式：`DB-EXP-NNN`、目的、命令、baseline 结果、改动后结果、结论（PASS/FAIL/REJECTED）。

| ID         | 目的                                           | 命令                                                            | Baseline                                                                               | After                                                                                                          | 结论 | Phase |
| ---------- | ---------------------------------------------- | --------------------------------------------------------------- | -------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- | ---- | ----- |
| DB-EXP-001 | SQLite 连接并发与 `sqlite3_interrupt` 线程模型 | `cargo test -p database_core cancel_stops_a_long_running_query` | `sqlez::Connection` 非 Sync；`sqlite3_interrupt` 允许跨线程中断进行中的 `sqlite3_step` | `Mutex` 串行化单连接，控制线程 interrupt 后 1 亿行递归 CTE 的 stream 在 5s 内关闭，取消状态 `Cancelled` 非错误 | PASS | 1     |
| DB-EXP-002 | 大结果集窗口化拉取的内存上限                   | 待定                                                            |                                                                                        |                                                                                                                |      | 4     |
| DB-EXP-003 | 扩展子进程协议跨平台换行/编码/超时             | 待定                                                            |                                                                                        |                                                                                                                |      | 6     |

## 决策变更记录 {#decision-changes}

执行期若需变更 [decisions.md](./decisions.md) 的冻结决策，先在此登记理由与影响，
再修改 decisions.md，最后改实现。

| 决策                 | 原结论                                     | 新结论                                                                                                                                                        | 理由                                                                                                                                                                                                                                                                                       | 影响阶段      | 日期       |
| -------------------- | ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------- | ---------- |
| D-02（事实前提修正） | 内建 SQLite 复用 `crates/sqlez` 的连接封装 | 复用其底层 bundled `libsqlite3-sys` FFI，在 `database_core` 自建专用连接封装；仍不新增第三方 SQLite crate、不复用/不改动 `crates/db`、`crates/sqlez` 保持不变 | Phase 0/1 源码复核：`sqlez::Connection` 非 Sync、`open` 是 `pub(crate)`、`open_file` 失败静默回退内存库、`step` 把 `SQLITE_INTERRUPT` 当错误、`map` 全量物化、`Column` 是编译期静态映射。与 D-07 运行时 `Value`、D-08 取消非错误、窗口化 `RowStream`、测试连接错误分类、显式事务结构性冲突 | 1（及 3/4/5） | 2026-11-04 |

## 提交记录 {#commits}

| SHA                                            | 提交信息                                                              | Phase | 验证                           | 备注                            |
| ---------------------------------------------- | --------------------------------------------------------------------- | ----- | ------------------------------ | ------------------------------- |
| `f2a037d404cc396581e24822d34cf2a885b02bc4`     | `docs: add database explorer plan`                                    | 0     | prettier PASS                  | 在 `main` 上，worktree 前置条件 |
| `9a1e9179af1d6ac8e51c2905dfed88dc2420cb82`     | `docs(database): record phase 0 baseline and ledger`                  | 0     | 见 baseline-validation 表      | Phase 0 账本定稿                |
| `812e4ae4be42401a9041e9a0768c6bc3dcae69ee`     | `feat(database_core): add domain model and sql analysis`              | 1     | check/test/clippy/fmt PASS     | 工作项 1                        |
| 本次（见 `git log -1` 于 `database-explorer`） | `feat(database_core): add driver protocol and built-in sqlite driver` | 1     | check/test(32)/clippy/fmt PASS | 工作项 2，DB-EXP-001 PASS       |

## Phase 1 工作项日志 {#phase-1-log}

每个工作项即时记录：baseline、改动、验证命令与 exit status、结论、提交 SHA、下一步。

### 工作项 1 — crate 骨架、领域模型、SQL 分析、错误分类

- **Baseline**：`database-explorer` @ `9a1e9179af`（Phase 0 账本定稿后）。
- **改动**：
  - 根 `Cargo.toml`：`members` 与 `[workspace.dependencies]` 新增
    `database_core` / `database_ui`（按字母序，不改其它行）。
  - 新建 `crates/database_core`（`[lib] path = "src/database_core.rs"`，无 `mod.rs`）：
    - `error.rs`：`DatabaseErrorKind`（connection/authentication/syntax/not_found/
      permission/cancelled/constraint/timeout/driver/other）+ `DatabaseError`，
      供 UI 按 kind 选 i18n 文案，不按错误文本匹配（不变量 8）。
    - `model.rs`：`Value` 全类型（D-07，BLOB 十六进制摘要 + 长度、NULL 与类型化数值
      区分、类型化排序键与导出文本）、`ColumnType::from_declared`（SQLite 亲和规则）、
      `Column`/`Row`、独立 `ResultSet`（状态机 Running/Ready/Cancelled/Failed/Closed、
      窗口行预算、pending edits、可编辑性需 capability + 主键，D-06）、`Capabilities`
      （能力声明，sqlite()/read_only()，不变量 1）、schema 树类型
      （`Catalog`/`Schema`/`Table`/`TableSchema`/`PrimaryKey`/`Index`/`ForeignKey`/
      `TableRef`/`ObjectKind`）、`ConnectionConfig`（仅非敏感字段 + `credential_key()`）、
      `DataSource`（`File`/`Server`/`InMemory`，路径为 `PathBuf` 不假设本地，D-10）、
      `ConnectionStatus`（运行时状态，不持久化，D-09）。
    - `sql.rs`：`split_statements`（顶层分号切分，跳过字符串/引号标识符/`--`/`/* */`
      注释、转义引号、保留 byte 偏移）+ `classify`（带括号深度的顶层 WHERE 检测、
      DDL/无 WHERE UPDATE/DELETE 二次确认分类，D-05）。
  - 新建 `crates/database_ui` 最小骨架（Cargo.toml + 空 `init`），使 workspace 可解析；
    Panel/Item 接线留待后续工作项。
  - `decisions.md`：D-02 事实前提修正（见决策变更记录），理由与影响已登记。
- **验证命令与 exit status**：
  - `cargo check -p database_core` → `CHECK_EXIT=0`
  - `cargo test -p database_core` → `TEST_EXIT=0`（21 passed；初版 1 例断言写错，修正后
    全绿：`ignores_semicolons_in_strings_and_comments` 前导注释是 trivia，不属于语句文本）
  - `cargo clippy -p database_core --all-targets -- -D warnings` → `CLIPPY_EXIT=0`
    （修了 `format!` 无参、`range_plus_one`、`useless_vec` 三处 lint）
  - `cargo fmt --all -- --check` → `FMT_EXIT=0`
- **结论**：PASS（核心领域逻辑）。
- **提交 SHA**：`812e4ae4be42401a9041e9a0768c6bc3dcae69ee`
  （`feat(database_core): add domain model and sql analysis`）。
- **下一步**：工作项 2（已完成，见下）。

### 工作项 2 — driver 协议与内建 SQLite 驱动

- **Baseline**：`database-explorer` @ `812e4ae4be`（工作项 1 提交后）。
- **改动**：
  - `crates/database_core/src/driver.rs`：
    - `DatabaseDriver` / `DriverConnection` trait（`#[async_trait::async_trait]`），
      `capabilities()` 是 UI 动作可用性唯一来源，禁止按 `id()` 分支（不变量 1）。
    - `QueryHandle::cancel()` **同步**且幂等（D-08），`Interrupt` trait 让 SQLite 走
      `sqlite3_interrupt`、扩展驱动走 `cancel` 帧；取消是 `Cancelled` 状态非错误。
    - `RowStream`：`async-channel::bounded(256)` 有界流式行，接收端 drop 即取消、
      不留孤儿任务（不变量 9）。
    - `ExecutedQuery` / `QueryMetadata` / `ExplainPlan` / `ExplainNode` /
      `TransactionId` / `DriverResult`。
  - `crates/database_core/src/sqlite.rs`（内建驱动，D-02 修订后的 FFI 自建封装）：
    - `SqliteHandle`：`Mutex<*mut sqlite3>` 串行化访问 + `unsafe impl Send/Sync`，
      `interrupt_ptr: usize` 供跨线程 `sqlite3_interrupt`；`Drop` 关闭连接。
    - `sqlite3_open_v2` 用 READWRITE|URI|NOMUTEX（**不 CREATE**），文件不存在返回
      `Connection` 分类错误，不静默建空库（修正 `sqlez::open_file` 的问题）。
    - schema 实查：`sqlite_master` 列表 + `PRAGMA table_info` / `foreign_key_list` /
      `index_list` / `index_info`，`quote_identifier` 防注入，列声明类型映射
      `ColumnType`，主键/索引/外键组装为 `TableSchema`。
    - `execute`：prepare + 参数绑定（`?N` 占位杜绝注入）+ 运行时 `Value` 列读取
      （`sqlite3_column_type` → Integer/Real/Text/Blob/Null），在 `std::thread` 上
      持连接锁循环 `sqlite3_step` 填充有界 channel；接收端关闭或取消标志触发
      `sqlite3_interrupt` 让 step 迅速返回。
    - `begin`/`commit`/`rollback`（事务）、`explain`（`EXPLAIN QUERY PLAN` →
      `ExplainPlan` 节点树）、`test`、`close`（`closed` 标志防 double-close）。
    - `classify_open_error` / `classify_step_error` 把 SQLite 错误码映射为
      `DatabaseErrorKind`（不按文本匹配，不变量 8）。
  - `Cargo.toml` 增 `async-trait` / `futures` 依赖；`database_core.rs` 导出 sqlite 类型。
  - 测试：schema 实查（列/类型/主键/索引）、类型化流行、参数绑定防注入、
    事务 commit/rollback、EXPLAIN、`cancel_stops_a_long_running_query`（DB-EXP-001）。
- **验证命令与 exit status**：
  - `cargo check -p database_core` → `CHECK_EXIT=0`
  - `cargo test -p database_core` → `TEST_EXIT=0`（32 passed; 0 failed）
  - `cargo clippy -p database_core --all-targets -- -D warnings` → `CLIPPY_EXIT=0`
    （修了 `multiple_unsafe_ops_per_block` 拆分、`manual_let_else`、
    `needless_lifetimes`、`redundant_clone`、`panic_in_if`、unreachable pattern、
    `Debug` derive 等 lint）
  - `cargo fmt --all -- --check` → `FMT_EXIT=0`
- **DB-EXP-001 结论（PASS）**：SQLite 单连接以 `Mutex` 串行化；`sqlite3_interrupt`
  可从控制线程调用并使 streaming 线程上的 `sqlite3_step` 返回 `SQLITE_INTERRUPT`。
  测试用 1 亿行递归 CTE，等到首行产出后 cancel，stream 在 5s 内关闭且状态为
  `Cancelled` 非错误。取消标志 + interrupt 双路径覆盖（线程未启动 vs 已 step）。
- **结论**：PASS（协议 + 内建 SQLite 驱动，纯逻辑可单测）。
- **提交 SHA**：见 `git log -1` 于 `database-explorer`
  （`feat(database_core): add driver protocol and built-in sqlite driver`）。
- **下一步**：工作项 3 —— `ConnectionManager`（凭据注入、状态机、连接生命周期
  owner）+ `QuerySession`（执行/取消/事务/pending edits），随后 `database_ui` 的
  Item/Panel 接线与 `main.rs` 的 `init` 注册（Phase 1 验收要求能打开真实 `.db` 文件）。
- **遗留风险（记录，非阻塞）**：`close()` 与进行中 streaming 并发时存在
  use-after-free 窗口；由 Phase 2/3 的 `ConnectionManager` 保证「close 前先 cancel
  全部查询并等待其 owner 结束」来消除，driver 层不单独持有查询 task。

## 完成定义核对 {#done-checklist}

收尾（Phase 13）时逐条核对[实现计划第 3 节](./implementation-plan.md#done)的 12 条与
第 7 节的 10 条不变量，记录证据位置。

| #   | 完成定义                         | 状态 | 证据 |
| --- | -------------------------------- | ---- | ---- |
| 1   | 阶段全完成、条件项有结论         |      |      |
| 2   | SQLite schema 实查浏览           |      |      |
| 3   | 查询执行/取消/结果集导出         |      |      |
| 4   | 编辑/事务/二次确认               |      |      |
| 5   | 凭据与连接配置                   |      |      |
| 6   | trait + 非 SQLite 扩展驱动演示   |      |      |
| 7   | ER/EXPLAIN/历史/Notebook/MCP     |      |      |
| 8   | csv_preview 抽取无回归、依赖方向 |      |      |
| 9   | i18n 双 catalog 一致             |      |      |
| 10  | 无坏味道与重复实现               |      |      |
| 11  | 全量门禁通过或已登记既有失败     |      |      |
| 12  | 账本完整                         |      |      |
