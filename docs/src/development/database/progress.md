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
- Current HEAD: `database-explorer` @ 本次提交（见 `git log -1`；Phase 7 为
  `d90e2ff59c`，Phase 6 为 `10756066b8`）。代码基线 `3af1429a83` 已
  `merge-base --is-ancestor` 验证包含。
- Branch/worktree: `database-explorer` @
  `/home/begonia/Documents/Github/Xero-Team/ZZZ-database-explorer`
- Active phase: `Phase 9`
- Last completed phase: `Phase 8`
- Blocking issue: `None`
- Next action: 进入 Phase 9——查询历史持久化与检索、历史重跑，且不记录结果集与凭据。

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
| 1     | 骨架与 SQLite 只读浏览   | PASS        | `05f659d46bfd` + `9e00c0cbb7`                    | check/test/clippy/fmt/main.rs check PASS       | 见 [Phase 1 日志](#phase-1-log)     |
| 2     | 连接管理与凭据           | PASS        | `9322d438571e`                                   | check/test/clippy/xtask check/fmt PASS         | 见 [Phase 2 日志](#phase-2-log)     |
| 3     | 查询执行与取消           | PASS        | `0e321d7522`                                     | check/test/clippy/xtask check/fmt PASS         | 见 [Phase 3 日志](#phase-3-log)     |
| 4     | 结果集视图               | PASS        | 本次提交（见 `git log -1`）                      | check/test/clippy/xtask check/fmt PASS         | 见 [Phase 4 日志](#phase-4-log)     |
| 5     | 编辑、事务与确认         | PASS        | 本次提交（见 `git log -1`）                      | check/test/clippy/xtask check/fmt PASS         | 见 [Phase 5 日志](#phase-5-log)     |
| 6     | 扩展驱动协议             | PASS        | `10756066b8`                                     | check/test/clippy/xtask check/fmt PASS         | 见 [Phase 6 日志](#phase-6-log)     |
| 7     | ER 图与导入导出          | PASS        | `d90e2ff59c`                                     | check/test/clippy/xtask check/fmt PASS         | 见 [Phase 7 日志](#phase-7-log)     |
| 8     | Visual EXPLAIN           | PASS        | 本次提交（见 `git log -1`）                      | check/test/clippy/xtask check/fmt PASS         | 见 [Phase 8 日志](#phase-8-log)     |
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

| SHA                                            | 提交信息                                                                | Phase | 验证                             | 备注                            |
| ---------------------------------------------- | ----------------------------------------------------------------------- | ----- | -------------------------------- | ------------------------------- |
| `f2a037d404cc396581e24822d34cf2a885b02bc4`     | `docs: add database explorer plan`                                      | 0     | prettier PASS                    | 在 `main` 上，worktree 前置条件 |
| `9a1e9179af1d6ac8e51c2905dfed88dc2420cb82`     | `docs(database): record phase 0 baseline and ledger`                    | 0     | 见 baseline-validation 表        | Phase 0 账本定稿                |
| `812e4ae4be42401a9041e9a0768c6bc3dcae69ee`     | `feat(database_core): add domain model and sql analysis`                | 1     | check/test/clippy/fmt PASS       | 工作项 1                        |
| `3f42c8244ca427e07c6f1604dc9631ad4e80fd21`     | `feat(database_core): add driver protocol and built-in sqlite driver`   | 1     | check/test(32)/clippy/fmt PASS   | 工作项 2，DB-EXP-001 PASS       |
| `05f659d46bfd5b273e256a6dd37121477c2d014a`     | `feat(database_ui): wire sqlite browsing panel and result view`         | 1     | check/test(41)/clippy/fmt PASS   | 工作项 3，Phase 1 完成          |
| `9e00c0cbb78d84bdd99324f87cbe324590497502`     | `fix(database_core): surface statement errors from the row stream`      | 1     | check/test(43)/clippy/fmt PASS   | 工作项 3 缺陷修复               |
| `9322d438571e5deaef8a6bb229e7bcfc07c14b3b`     | `feat(database_ui): save connections in settings with keychain secrets` | 2     | check/test/clippy/xtask/fmt PASS | Phase 2 完成                    |
| `0e321d7522a48873306290b68a0d138fde560da2`     | `feat(database_ui): run sql from the database editor`                   | 3     | check/test/clippy/xtask/fmt PASS | Phase 3 完成                    |
| 本次（见 `git log -1` 于 `database-explorer`） | `feat(database_ui): virtualize the result grid`                         | 4     | check/test/clippy/xtask/fmt PASS | Phase 4 完成                    |
| 本次（见 `git log -1` 于 `database-explorer`） | `feat(database_ui): edit rows inside explicit transactions`             | 5     | check/test/clippy/xtask/fmt PASS | Phase 5 完成                    |
| `10756066b89a478da5e6bc768518b082a5bd4fa8`     | `feat(database_core): run extension database drivers as subprocesses`   | 6     | check/test/clippy/xtask/fmt PASS | Phase 6 完成                    |
| `d90e2ff59c7fa12eb4f6e9e2df189ece12fafa9f`     | `feat(database_ui): diagram schemas and import csv in transactions`     | 7     | check/test/clippy/xtask/fmt PASS | Phase 7 完成                    |
| 本次（见 `git log -1` 于 `database-explorer`） | `feat(database_ui): show the query plan as a tree`                      | 8     | check/test/clippy/xtask/fmt PASS | Phase 8 完成                    |

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
- **下一步**：工作项 3（已完成，见下）。

### 工作项 3 — 连接管理、查询会话与 GPUI 接线

- **Baseline**：`database-explorer` @ `3f42c8244c`（工作项 2 提交后）。
- **改动**：
  - `crates/database_core/src/driver.rs`：`QueryHandle` 增稳定 `id`（`NEXT_QUERY_ID`），
    供 `ConnectionManager` 跟踪进行中的查询；`DatabaseDriver::connect` 增
    `secret: Option<&[u8]>` 参数，凭据由调用方按次注入，不进入 `ConnectionConfig`
    （不变量 3、D-09）。
  - `crates/database_core/src/connection.rs`（`ConnectionManager`）：
    - 驱动注册表按 `driver.id()` 存 `capabilities`，UI 能力来源是声明而非 id 分支
      （不变量 1）；`capabilities_for` 返回 `Option<Capabilities>`。
    - `upsert_config`/`configs`/`status` 只保存非敏感字段；状态机
      Disconnected/Connecting/Connected/Querying/Disconnected 为运行时状态，不持久化
      （不变量 4）。
    - `connect(id, secret)` 幂等：已连接直接复用；失败回落到 Disconnected 并返回
      typed `DatabaseError`（不按文本匹配）。`test_connection` 仅连接后调用 `test()`。
    - `track_query`/`finish_query` 记录进行中查询；`disconnect` 先取出并 `cancel`
      全部在飞查询，再 `close` 原生连接，消除工作项 2 记录的 close/step 竞态。
  - `crates/database_core/src/session.rs`（`QuerySession`）：`execute` 把
    `RowStream` 填进有界窗口（`DEFAULT_WINDOW_LIMIT`），窗口满即 `cancel` 并标记
    Cancelled（D-08，取消不是错误）；`begin_transaction_async`/`commit`/`rollback`
    由会话独占事务生命周期；`build_update_statements` 用 `?N` 占位从 pending edits
    生成参数化 DML（防注入）；`export_csv` 复用 `Value::display`。
  - `crates/database_ui`：
    - `database_ui.rs`：`DatabaseState` 全局（`ConnectionManager` + keychain helper），
      `DatabaseState::manager/credentials`；`init` 注册 `SqliteDriver`、`OpenDatabase`/
      `ToggleDatabasePanel` action、`ConnectionPanel` 与 `main.rs` 接线。
    - `connection_panel.rs`：`Panel` 列出已保存连接及状态；`OpenDatabase` 经
      `cx.prompt_for_paths` 选文件，`config_for_path` 生成非敏感配置
      （id = `file:<path>`，`DataSource::File`），再在该连接上开 `ResultSetView`。
    - `result_set_view.rs`：`Item` 从实查 `sqlite_master` 的 schema 树渲染 catalog/
      schema/table，点击表再查 `PRAGMA table_info`/`index_list`/`foreign_key_list`
      展示列、主键、索引、外键；头部显示连接状态与是否可编辑（能力声明）。
    - `credentials.rs`：`KeychainCredentials` 只持 `Arc<dyn CredentialsProvider>`
      （`Send + Sync`），每次 `read/write/delete` 由调用方传入 `AsyncApp`；口令
      不写入配置、日志或错误信息。
  - `crates/zzz`：`main.rs` 增 `database_ui::init(cx)`，`Cargo.toml` 增
    `database_ui.workspace = true`；根 `Cargo.lock` 随新依赖更新。
  - i18n：`database.*` 共 15 键加入 `assets/locales/en.json` 与 `zh-CN.json`
    （各 3860 条，key 集合一致，fallback 与 en 文案相同）。
- **验证命令与 exit status**：
  - `cargo check -p database_ui` → `CHECK_EXIT=0`
  - `cargo clippy -p database_core -p database_ui --all-targets -- -D warnings` →
    `CLIPPY_EXIT=0`（修了 `arc_with_non_send_sync`——helper 改为只持 provider；
    `manual_let_else`/`map_or_else`；`empty_line_after_doc_comment`）
  - `cargo test -p database_core -p database_ui` → `TEST_EXIT=0`（core 40 passed、
    ui 1 passed）。新增 `connection::tests::opens_a_real_file_and_describes_schema`：
    在临时目录建真实 `.db`，经 `ConnectionManager` 连接、建表/索引/外键，断言
    catalogs、主键、索引、外键与断开后的状态，覆盖 Phase 1 验收路径。
  - `cargo check -p zzz` → `CHECK_EXIT=0`（1m12s，验证 `init` 与依赖接线）
  - `cargo fmt --all -- --check` → `FMT_EXIT=0`
- **实现决策（记录）**：本 fork 没有「路径 → 视图」注册表（`ProjectItemKind`
  仅用于预览标签恢复，项目条目类型只有文本），`image_viewer` 也只注册
  `ProjectItem` 而无按扩展名的双击路由。因此 `.db` 入口按既有 `csv_preview`
  的 action 模式实现（命令面板 `database: open database`），而不是拦截双击；
  若后续阶段需要双击，须先引入路径-视图注册机制并单独立项。
- **结论**：PASS。Phase 1 验收（打开真实 SQLite 文件并浏览表、列、主键、索引、
  外键）由 `opens_a_real_file_and_describes_schema` 与 UI schema 树共同满足。
- **提交 SHA**：见 `git log -1` 于 `database-explorer`
  （`feat(database_ui): wire sqlite browsing panel and result view`）。
- **遗留风险（已修复）**：`RowStream` 原先只承载 `Row`，`sqlite3_step` 的非 ROW/DONE
  错误（约束冲突等）在 `stream_rows` 中被静默丢弃；Phase 3/5 的错误与编辑路径需要
  它，故在工作项 3 提交后立即单独立项修复（见下）。
- **下一步**：Phase 2 —— `database.connections` 设置 schema、连接对话框、测试连接、
  凭据写入/删除与连接状态机 UI。

### 工作项 3 后续缺陷修复 — 流式语句错误不再被吞掉

- **Baseline**：`database-explorer` @ `05f659d46bfd`（工作项 3 提交后）。
- **发现**：工作项 3 的验收测试要求「消费完流再发下一条语句」，由此发现
  `stream_rows` 对 `sqlite3_step` 的其它返回码直接 `break`，`read_row` 解码失败也
  直接 `break`。对无结果行的 DML 而言，约束冲突、只读库等失败会表现为「流正常
  结束」，`QuerySession::execute` 返回 `Ok`，与不变量 8（typed 错误）冲突，也会
  让 Phase 5 的编辑提交在失败时静默成功。
- **改动**：
  - `driver.rs`：`RowStream` 元素类型改为 `DriverResult<Row>`；错误是流的最后一项，
    之后流结束。取消仍然是「流直接结束」，不产生错误（D-08 不变）。
  - `sqlite.rs`：`stream_rows` 在 `read_row` 失败与非 ROW/DONE/INTERRUPT 返回码时
    发送 classified `DatabaseError` 再结束；接收端已关闭则仅结束（无处可报）。
  - `session.rs`：`execute` 收到错误项时 `mark_failed()`、结算 `track_query`、
    保留已读行并返回该错误；`apply_pending_edits` 排空 DML 时用 `item?` 传播，
    失败不再继续 commit。
  - **同时修正错误分类**：`classify_step_error`/`classify_open_error` 先 `code & 0xff`
    取主返回码。此前扩展码（如 `SQLITE_CONSTRAINT_PRIMARYKEY` = 1555）全部落到
    `Other`，正是新增测试暴露的问题。
- **验证命令与 exit status**：
  - `cargo test -p database_core -p database_ui` → `TEST_EXIT=0`（core 42 passed、
    ui 1 passed）。新增 `row_stream_delivers_a_terminal_error`、
    `statement_errors_are_delivered_through_the_stream`（多行 INSERT 主键冲突 →
    `DatabaseErrorKind::Constraint`）。
  - `cargo clippy -p database_core -p database_ui --all-targets -- -D warnings` →
    `CLIPPY_EXIT=0`
  - `cargo fmt --all -- --check` → `FMT_EXIT=0`
  - `cargo check -p zzz` 未重跑：本次只改 `database_core` 内部流类型，`database_ui`
    在 clippy 中已完整编译；Phase 2 首次改动后重跑工作区检查。
- **结论**：PASS。取消语义、窗口预算与既有 42 项测试均未回退。
- **提交 SHA**：见 `git log -1` 于 `database-explorer`
  （`fix(database_core): surface statement errors from the row stream`）。
- **遗留风险（记录，非阻塞）**：`close()` 与进行中 streaming 并发时存在
  use-after-free 窗口；由 Phase 2/3 的 `ConnectionManager` 保证「close 前先 cancel
  全部查询并等待其 owner 结束」来消除，driver 层不单独持有查询 task。

## Phase 2 日志 {#phase-2-log}

### 连接管理与凭据

- **Baseline**：`database-explorer` @ `9e00c0cbb7`（Phase 1 缺陷修复后）。
- **改动**：
  - `crates/settings_content/src/database.rs`（新）：`DatabaseSettingsContent
{ connections }` 与 `DatabaseConnectionContent { id, name, driver, path?, host?,
port?, user?, database? }`，派生 `JsonSchema`/`MergeFrom`；字段有文档注释供
    schema 使用。`SettingsContent` 新增 `database: Option<DatabaseSettingsContent>`。
    结构体没有任何密码字段，口令在类型层面就无法进入设置（D-09）。
  - `crates/database_ui/src/connection_settings.rs`（新）：`DatabaseSettings`
    实现 `Settings` + `RegisterSetting`，把 settings 条目转成 `ConnectionConfig`；
    目标缺失的条目跳过并 `log::warn!`，不让整个设置加载失败。另有
    `connection_to_content`、`next_connection_id`（稳定、与显示名解耦）、
    `config_for_file_path`（同一路径复用已有连接）、`persist_connection`
    （`update_settings_file` 是唯一持久写入路径）。
  - `crates/database_core`：
    - `Capabilities::requires_credentials`（serde 默认 false）：对话框是否显示口令
      输入由驱动声明决定，不按 driver id 分支（不变量 1）。
    - `ConnectionManager::sync_configs`：用设置中的集合同步运行时注册表——移除已删除
      的连接，重置配置变化的连接（先 cancel 在飞查询再丢弃原生连接），配置未变的
      活连接保持状态与查询。
    - `ConnectionManager::test_config`：不注册、不落盘地验证一份配置；连接始终关闭，
      `test()` 与 `close()` 的错误都不吞。
  - `crates/database_ui/src/connection_dialog.rs`（新）：`ModalView` 连接对话框。
    File 连接显示名称与路径（平台文件选择器），Server 连接显示 host/port/user/
    database；仅当驱动声明 `requires_credentials` 时显示被掩码的口令输入与
    “只存钥匙串”说明。`Test Connection` 走 `test_config` 并把 typed 错误映射为
    `database.error.*` 文案；`Save` 先把口令写入 keychain（仅在输入了新口令时），
    再写设置并同步运行时；`Delete` 先断开、再删 keychain 与设置。
  - `crates/database_ui/src/connection_panel.rs`：连接行显示
    `database.status.*` 与能力/驱动缺失提示，提供打开、连接/断开、编辑按钮；
    头部“添加连接”分发 `database: add connection`；打开 `.db` 时复用同路径的已保存
    连接，否则先持久化再打开，避免面板与设置不一致。
  - `crates/database_ui/src/database_ui.rs`：初始化与每次 `SettingsStore` 变化时调用
    `sync_from_settings`；新增 `AddDatabaseConnection` action。
  - `crates/settings_ui/src/pages/database_connections_page.rs`（新）与
    `page_data.rs` 的 Network 页 `SubPageLink`：设置界面显示已保存连接（驱动与
    目标、只读摘要），并提供 “Edit in JSON”（`in_json: true`）。增删改统一走数据库
    面板，使 keychain 与 settings 各只写一次。
  - `crates/settings/src/vscode_import.rs`：导入 VSCode 设置时补 `database: None`。
  - **Phase 1 遗漏修复**：`crates/database_core`、`crates/database_ui` 缺少
    `LICENSE-AGPL` 符号链接，Phase 1 只跑了编译器检查、未跑 `cargo xtask check`。
    已补链接，`cargo xtask check` 现在 6/6 通过。
- **验证命令与 exit status**：
  - `cargo check -p database_ui` → `CHECK_EXIT=0`；`cargo check -p settings_ui` →
    `CHECK_EXIT=0`；`cargo check -p zzz` → `CHECK_EXIT=0`（26.17s）
  - `cargo test -p database_core -p database_ui` → `TEST_EXIT=0`（core 45、ui 4）。
    新增：`sync_configs_adds_removes_and_resets_changed_connections`、
    `test_config_leaves_the_registry_untouched`、
    `rejected_credentials_are_classified_and_never_stored`、
    设置条目往返、同路径复用、id 生成。
  - `cargo test -p settings -p settings_content` → `TEST_EXIT=0`
    （39 + 47 passed、1 ignored、2 integration）
  - `cargo clippy -p database_core -p database_ui --all-targets -- -D warnings` →
    `CLIPPY_EXIT=0`
  - `cargo xtask check` → `XCHECK_EXIT=0`（6 checks：unfinished-work、keymaps、
    crate licenses、philosophy、upstream ledger、workflows）
  - `cargo fmt --all -- --check` → `FMT_EXIT=0`
- **实现决策（记录）**：
  - 内建驱动只有 SQLite，对话框实际只创建 File 连接；schema 与对话框已经按
    `DataSource::Server` 分支，server 驱动在 Phase 6 接入后无需改数据结构。
  - “断开时事务提示”按计划留给 Phase 5（与编辑/事务联动）；当前 `disconnect`
    仍先 cancel 全部查询再 close。
  - 验收项“错误口令给出分类文案”在 manager 边界用 `requires_credentials` 的测试驱动
    覆盖（错误 kind = Authentication）+ typed 文案映射；内建 SQLite 无口令，网络认证
    要等 Phase 6 的扩展驱动才能端到端验证。此项在 Phase 6 回归。
- **结论**：PASS。设置默认值不含任何远程主机（`database` 默认为空），
  `script/check-philosophy` 通过。
- **提交 SHA**：见 `git log -1` 于 `database-explorer`
  （`feat(database_ui): save connections in settings with keychain secrets`）。
- **下一步**：Phase 3 —— SQL 编辑器 action（run statement / run selection）与默认键位、
  语句切分、`QuerySession` 执行/取消/错误文案、多查询标签页接线。

## Phase 3 日志 {#phase-3-log}

### SQL 执行、取消与查询标签页

- **Baseline**：`database-explorer` @ `9322d438571e`（Phase 2 提交后）。
- **改动**：
  - `crates/database_core/src/sql.rs`：新增 `statement_at(sql, cursor)`——优先返回光标
    所在的语句，光标落在语句之间的空白时回退到其前面的语句；纯函数，含单元测试。
  - `crates/database_core/src/connection.rs`：新增
    `ConnectionManager::cancel_queries(id)`，取消该连接所有已跟踪的查询但不断开连接；
    这样结果视图在 `QuerySession` 持有查询句柄、正在 await 行流时仍能响应用户取消。
    附 1 亿行递归 CTE 的取消测试（恢复 `Connected` 状态）。
  - `crates/database_core/src/session.rs`：新增三条执行路径测试——
    `execute_fills_a_ready_result_set`（Ready + 行值）、
    `execute_surfaces_classified_errors`（约束冲突 → `Constraint`，结果集 `Failed`）、
    `window_budget_marks_the_result_cancelled`（超窗口预算 → `Cancelled`，
    行数 = `DEFAULT_WINDOW_LIMIT`）。
  - `crates/database_ui/src/result_set_view.rs`：
    - 新增多行 `editor::Editor`（key context `DatabaseEditor`，占位提示），连接成功后
      创建并保存 `QuerySession`。
    - `database::RunStatement` / `RunSelection` / `CancelQuery` 注册在编辑器实体上，
      避免动作落到其它编辑器；`sql_to_run` 是纯函数（Statement 选中优先、否则当前
      语句；Selection 必须有选中内容），在 UI crate 内单测。
    - 查询标签页：每个标签保存 SQL、结果集与状态消息；切换标签把当前编辑器文本存回
      旧标签，支持新增标签；头部提供 Run / Run Selection / Cancel Query。
    - 结果先以简单表格渲染（最多 200 行并给出 `行 × 列` 形状与截断提示），Phase 4 用
      虚拟化网格替换；错误文案统一走 `describe_error`（typed kind → i18n）。
    - 取消按钮调用 `manager.cancel_queries(connection_id)`；结果状态仍由
      `ResultSetState` 决定，`Cancelled` 不显示为错误（D-08）。
  - 默认键位（`assets/keymaps/default-{linux,windows,macos}.json`）：
    `DatabaseEditor` 上下文内 `ctrl-enter`（macOS `cmd-enter`）运行当前语句、
    `ctrl-shift-enter` 运行选中、`ctrl-shift-c` 取消查询。该上下文比 `Editor` 更具体，
    不影响普通编辑器 `ctrl-enter = editor::Newline`；`cargo xtask check` 的 keymaps
    检查通过。
  - i18n：新增 15 个 `database.query.*` 键（en/zh 各 3917 条，key 集合一致）。
- **验证命令与 exit status**：
  - `cargo check -p database_ui` → `CHECK_EXIT=0`
  - `cargo test -p database_core -p database_ui` → `TEST_EXIT=0`（core 51、ui 8）
  - `cargo clippy -p database_core -p database_ui --all-targets -- -D warnings` →
    `CLIPPY_EXIT=0`
  - `cargo xtask check` → `XCHECK_EXIT=0`（含 keymaps 校验）
  - `cargo check -p zzz` → `ZCHECK_EXIT=0`
  - `cargo fmt --all -- --check` → `FMT_EXIT=0`
- **实现决策（记录）**：
  - SQL 编辑器是普通多行编辑器（未接 SQL 语言/高亮）；Phase 3 只要求执行与取消，
    高亮属于后续可选项，不阻塞验收。
  - 窗口预算由 `QuerySession` 在 10 000 行处主动取消并标记 `Cancelled`，这是
    “取消状态而非错误”的一个真实例子；用户取消走 `manager.cancel_queries`。
  - 已知分类限制：SQLite 对「表/列不存在」与语法错误都返回 `SQLITE_ERROR`，当前统一
    映射为 `Syntax`。Phase 5 编辑/确认路径前需在 SQLite 驱动层用 `sqlite3_error_offset`
    或扩展错误码进一步区分（属驱动适配，不是 UI 文本匹配）。
- **结论**：PASS。执行、错误、取消三条路径均有测试；长查询可在 handle 跟踪后取消且
  状态为 `Cancelled`；默认键位与既有键位不冲突。
- **提交 SHA**：见 `git log -1` 于 `database-explorer`
  （`feat(database_ui): run sql from the database editor`）。
- **下一步**：Phase 4 —— 虚拟化结果网格（窗口拉取、虚拟滚动、类型化渲染、截断诊断），
  替换当前的简单表格。

## Phase 4 日志 {#phase-4-log}

### 虚拟化结果网格与共享单元格交互

- **Baseline**：`database-explorer` @ `0e321d7522`（Phase 3 提交后）。
- **改动**：
  - `crates/ui/src/components/table_cell.rs`（新）：共享的
    `CopyableTableCell` 构建器（内边距、换行、横向对齐、文本颜色、右键复制）与
    `with_copy_on_right_click`。把原本只存在于 `csv_preview` 的单元格与复制交互抽到
    `ui` 层，两处渲染由此共用同一实现。
  - `crates/ui/src/components/table_header.rs`（新）：共享的
    `table_sort_button` 与 `TableSortDirection`（紧凑按钮、三个状态、i18n 提示）。
    `csv_preview` 的排序按钮改为调用它，排序状态机仍由各自表格拥有。
  - `crates/csv_preview/src/renderer/table_cell.rs`、`table_header.rs`：删除本地重复
    实现，改用 `ui` 的共享组件；`csv_preview` 的全部 16 项测试通过，无回归。
  - `crates/database_core/src/model.rs`：`ResultSet::truncation_note: Option<String>`
    改为 typed `truncation: Option<Truncation { row_limit }>`，让 UI 用 i18n 渲染截断
    文案，避免把英文诊断直接显示给用户。
  - `crates/database_core/src/session.rs`：
    - 区分「用户取消」与「触达行窗口预算」：预算触发时先 `cancel()` 停流，但结果集
      标记为 `Ready` + `truncation`，不再误报 `Cancelled`（D-08 仍适用于用户取消）。
      原 `window_budget_marks_the_result_cancelled` 测试改写为
      `window_budget_truncates_without_reporting_a_user_cancel`。
    - 新增 `ExportFormat { Csv, Markdown, Html }` 与
      `export_result_set(result_set, format, order)`：按显示顺序导出，Markdown 转义
      `|`/换行、HTML 转义 `&<>"'`，NULL 导出为空字段；`export_csv` 保留为恒等顺序的
      包装。新增 3 项导出测试。
  - `crates/database_ui/src/result_grid.rs`（新）：结果网格状态——可拖拽列宽
    （`ResizableColumnsState`，按列名与前 200 行估算初始宽度）、滚动/交互状态
    （`TableInteractionState`）、以及客户端排序（`Value::sort_key` 保留类型；NULL 升序
    在前、降序在后；同值保持原顺序）。`CellPresentation` 把 NULL/数值/BLOB/文本分类
    成可测试的展示规则，BLOB 只显示摘要而不内联二进制。
  - `crates/database_ui/src/result_set_view.rs`：用 `ui::Table` 的 `uniform_list`
    虚拟滚动替换 Phase 3 的 200 行静态表格；表头带排序按钮与右键复制列名，单元格
    右键复制（NULL 复制为空）；`TableAccessibility` 提供表名、列头与每个单元格的
    可访问标签；`Copy CSV` / `Copy Markdown` / `Copy HTML` 按当前显示顺序写入剪贴板；
    结果区显示形状、截断提示与「排序仅覆盖已加载行」的说明。
  - i18n：新增 12 个键（`database.grid.*`、`database.query.truncated`、
    `table.sort.*`），删除 4 个不再使用的键；en/zh 各 3925 条且 key 集合一致。
- **验证命令与 exit status**：
  - `cargo check -p database_core` → `CORE_EXIT=0`；`cargo check -p ui` →
    `UI_EXIT=0`；`cargo check -p csv_preview` → `CSV_EXIT=0`；
    `cargo check -p database_ui` → `DBUI_EXIT=0`
  - `cargo test -p database_core -p database_ui` → `TEST_EXIT=0`（core 54、ui 18）。
    新增：排序（数值/降序/NULL/同值）、排序状态循环、列宽边界、单元格分类
    （数值、NULL、BLOB 摘要、文本/日期）、三种导出与顺序。
  - `cargo test -p csv_preview` → `CSV_TEST_EXIT=0`（16 passed）
  - `cargo test -p ui` → `UI_TEST_EXIT=0`（77 + 41 passed）
  - `cargo test -p i18n` → `I18N_EXIT=0`（3 passed，两个 locale key 集合一致）
  - `cargo clippy -p database_core -p database_ui -p csv_preview -p ui --all-targets
-- -D warnings` → `CLIPPY_EXIT=0`
  - `cargo xtask check` → `XCHECK_EXIT=0`（6 checks）
  - `cargo check -p zzz` → `ZCHECK_EXIT=0`
  - `cargo fmt --all -- --check` → `FMT_EXIT=0`
- **实现决策（记录）**：
  - **「窗口化拉取」的边界**：当前驱动在 `sqlite3_step` 流式期间持有连接锁，若在
    两次拉取之间保留未消费的 `RowStream`，连接会被阻塞到下一次滚动，其它查询与
    `disconnect` 都无法进行。因此 `QuerySession` 在一次操作内拉满
    `DEFAULT_WINDOW_LIMIT`（10 000 行）后取消停流并释放连接；内存上限由该窗口保证，
    UI 用虚拟滚动只渲染可见行。若将来需要真正的分页，必须先把驱动改为分批
    prepare/step 并释放锁，不能只改 UI。
  - **排序范围**：排序在已加载窗口内进行，界面显式说明「在已加载的 N 行内排序」。
    不生成 `ORDER BY` 重新查询，因为列名可能重复/别名不确定，且换序会改变用户看到
    的语义。
  - 单元格复制 `display()` 所示文本；NULL 与导出一样复制为空字符串。数值右对齐，
    NULL 与 BLOB 用弱化颜色，BLOB 不展开二进制（D-07）。
  - 结果集以 `Arc<ResultSet>` 存放在标签页，渲染与可访问性闭包只克隆指针，避免每帧
    复制整窗数据。
- **结论**：PASS。大结果集内存有上限（10 000 行窗口 + 截断提示），结果集具备
  虚拟滚动、列宽拖拽、排序、单元格复制与三种格式导出；`csv_preview` 无回归；
  `csv_preview` 仍不依赖 `database_core`，共享代码只在 `ui`。
- **提交 SHA**：见 `git log -1` 于 `database-explorer`
  （`feat(database_ui): virtualize the result grid`）。
- **下一步**：Phase 5 —— pending edit 模型、参数化 DML、显式 `Commit`/`Rollback`、
  无 `WHERE` 的 `UPDATE`/`DELETE` 与 DDL 二次确认、事务未提交关闭提示。

## Phase 5 日志 {#phase-5-log}

### 编辑、事务与确认

- **Baseline**：`database-explorer` @ `57ff2dab1d`（Phase 4 提交后）。
- **改动**：
  - `crates/database_core/src/model.rs`：新增 `parse_edited_value(
column_type, text)` 与 `EditValueError`。按列类型解析整数/实数/布尔/日期/
    NUMERIC/JSON；BLOB 不可编辑。`NULL`（任意大小写）显式表示 SQL NULL；空文本对
    数值与日期类型表示 NULL，对文本类表示空字符串。
  - `crates/database_core/src/sql.rs`：新增 `editable_target(sql)`，只接受
    `SELECT ... FROM <table>` 的单表形状；拒绝 JOIN、逗号表列表、UNION/INTERSECT/
    EXCEPT、GROUP/HAVING、DISTINCT、子查询与 CTE。`SqlToken` 记录字节范围以解析
    限定表名（schema/catalog）与别名。
  - `crates/database_core/src/session.rs`：
    - `QuerySession` 新增 `applied_edits`、`applied_statements`、`applied_rows`。
      `pending_edits` 仍是用户可见改动的唯一来源；Apply 只把「尚未写入事务的增量」
      生成参数化 UPDATE，成功后记录到 `applied_edits`，因此再次 Apply 只发送增量。
    - `apply_pending_edits` 在 `SAVEPOINT zzz_edit_batch` 内批量执行；任一条失败则
      `ROLLBACK TO SAVEPOINT` + `RELEASE`，只撤销本批次并保留外层事务供修正后重试。
      未开启事务时先 `begin()`，事务与查询句柄都有明确 owner。
    - `commit` 只在所有 pending 编辑确实已应用时把值折入缓存行；`rollback` 清除
      已应用记录但不改缓存，界面立即回到原值。`commit_edits` = Apply + Commit；
      `rollback_edits` = 丢弃 pending + Rollback。
    - `discard_pending_edits` 在已有 Apply 时拒绝清除，避免界面值与事务内容矛盾。
    - `bind_table_after_query`：只有 `editable_target` 命中、驱动报告主键、且结果列
      包含全部主键时才授予编辑能力；否则结果集保持只读。
    - `execute_simple` 排空 DML 行流并在成功/失败两条路径都 `finish_query`，防止
      查询句柄泄漏。
  - `crates/database_core/src/sqlite.rs`：`describe_table` 对带 schema 的
    `TableRef` 使用 `PRAGMA "schema".table_info(...)` 等限定形式，避免把 attached
    schema 的同名表误当 `main` 表（否则编辑会定位到错误行）。
  - `crates/database_ui/src/result_set_view.rs`：
    - 单元格左键选中并在编辑栏显示行/列与单行 `Editor`（`DatabaseValueEditor >
Editor` 上下文：`enter` = Stage，`escape` = Cancel）；选中单元格高亮。
    - `Stage` 解析文本并 `stage_edit`；`Discard Staged`、`Apply`、`Commit`、
      `Rollback` 按钮与 `{} staged · {} statements applied · {} rows affected`
      计数。只读结果集显示不可编辑原因（无主键/驱动不支持）。
    - 运行无 `WHERE` 的 `UPDATE`/`DELETE` 与 DDL 前用平台
      `window.prompt` 二次确认；取消则不执行。
    - 有 pending/applied 编辑时拒绝运行新查询，要求先 Commit/Rollback，避免新结果集
      替换编辑归属。
    - `Item::is_dirty` = 存在未提交工作（pending、已 Apply、手开事务）；
      `can_save` 同步；`save` 执行 `commit_edits`（关闭提示的 Save 即提交）；
      `deactivated` 用 `background_spawn` 回滚，任务不随视图销毁而取消。
  - `crates/database_ui/src/database_ui.rs`：新增 `StageCellEdit`、`CancelCellEdit`
    action；三个平台键位文件增加 `DatabaseValueEditor > Editor`（Enter/Escape）。
  - i18n：新增 29 个 `database.edit.*` / `database.query.confirm_*` 键；en/zh 各
    3954 条且 key 集合一致，代码 fallback 与 `en.json` 完全一致。
- **验证命令与 exit status**：
  - `cargo check -p database_ui` → `EXIT=0`
  - `cargo test -p database_core -p database_ui` → `TEST_EXIT=0`（core 65、ui 18）
  - `cargo test -p database_core -p database_ui -p csv_preview -p i18n` →
    `TEST_EXIT=0`（csv 16、i18n 3）
  - `cargo clippy -p database_core -p database_ui -p csv_preview -p ui --all-targets
-- -D warnings` → `CLIPPY=0`
  - `cargo xtask check` → `XCHECK=0`（6 checks）
  - `cargo check -p zzz` → `ZCHECK=0`
  - `cargo fmt --all -- --check` → `FMT_EXIT=0`
  - `cd docs && npx prettier --check src/` → `PRETTIER_EXIT=0`
- **新增测试**：
  - 文件数据库双连接：Apply 未 Commit 时第二连接仍读到旧值；Commit 后读到新值。
  - Rollback 后数据不变、缓存值恢复、pending 清空。
  - 批次中第二条 UPDATE 触发 UNIQUE 约束：整个批次回滚，`applied_*` 保持 0，
    外层事务仍打开，pending 保留。
  - Apply 增量语义：重复 Apply 返回 0；Discard 在 Apply 后拒绝。
  - `editable_target` 接受简单/限定/别名 SELECT，拒绝 JOIN、逗号、GROUP、DISTINCT、
    UNION、子查询、CTE、非 SELECT；忽略字符串与注释中的关键字。
  - `bind_table_after_query` 对缺失主键列、JOIN、无主键表保持只读。
  - `parse_edited_value` 的类型解析、NULL/空值规则与错误分支。
  - 参数化 UPDATE 不含用户值（沿用 Phase 1 测试）。
- **实现决策（记录）**：
  - 二次确认使用平台 `window.prompt`（与 `Pane` 的未保存提示同一交互），不新增自定义
    modal；确认文案区分「无 WHERE 影响全表」与「DDL 结构变更」。
  - Commit 按钮走 `commit_edits`（Apply 剩余增量后提交），因此不会出现「已 Apply
    但 Commit 只提交旧批次」。
  - 编辑栏一次只编辑一个单元格；BLOB 不提供文本编辑，与 D-07 不内联二进制一致。
  - 只读结果集也显示一行原因说明，避免用户点击后才发现不可编辑。
- **结论**：PASS。不提交不落库、回滚数据不变、批次失败隔离、参数化 DML 与二次确认
  均有测试；关闭未提交事务会提示，Save = Commit，Don't Save = Rollback。
- **提交 SHA**：见 `git log -1` 于 `database-explorer`
  （`feat(database_ui): edit rows inside explicit transactions`）。
- **下一步**：Phase 6 —— 扩展清单 `database_drivers`、子进程协议、mock 驱动端到端、
  能力声明驱动 UI 可用性。

## Phase 6 日志 {#phase-6-log}

### 扩展清单与子进程驱动协议

- **Baseline**：`database-explorer` @ `0926c5cbaf`（Phase 5 提交后）。
- **改动**：
  - `crates/extension/src/extension_manifest.rs`：新增
    `database_drivers: Vec<DatabaseDriverManifestEntry>`（`[[database_drivers]]`），
    条目含 `id`、`command`（扩展目录内相对路径）、可选 `args`、`dialects` 与
    `capabilities`。能力缺省全部为 false（安全只读默认），UI 因此不会显示驱动
    做不到的动作。新增两条解析测试：声明值生效、缺省值保守。
  - `crates/extension_host/src/extension_host.rs`：新增
    `path_to_extension_dir(extension_id)`，让主机把清单里的相对命令解析为已安装扩展
    目录下的绝对路径；未新增 WIT resource。
  - `crates/database_core/src/driver.rs`：`DatabaseDriver::id` / `display_name`
    改为返回借用 `&str`（扩展驱动 id 是运行时字符串）；为 `ExplainPlan` /
    `ExplainNode` 增加 serde；`DriverConnection` 增加默认空实现的 `interrupt()`，
    供尚未返回查询句柄的子进程查询取消。
  - `crates/database_core/src/connection.rs`：`cancel_queries` 在取消已跟踪句柄后
    调用 `connection.interrupt()`；子进程驱动的 `execute` 在返回句柄前等待元数据，
    只有连接级 interrupt 能中止它。
  - `crates/database_core/src/session.rs`：驱动在返回句柄前报告 `Cancelled`
    （或流最后一项为 `Cancelled`）时，结果集进入 `Cancelled` 且 `execute` 返回
    `Ok(())`，与 D-08 一致，不把用户取消当失败。
  - `crates/database_core/src/extension_driver.rs`（新）：
    - `ExtensionDriverConfig`（id、display name、绝对命令、args、capabilities、
      控制/执行超时）；`ExtensionDriver` 实现 `DatabaseDriver`。
    - 每个连接一个 `smol::process::Command` 子进程；stdout 由专用线程按行解析
      JSON 并按请求 id 分发到 `async_channel`，stderr 逐行写日志。
    - 请求超时：控制请求 30s、`execute` 300s（可由 config 覆盖）；超时会移除挂起
      项并向子进程发 `cancel`，返回 `DatabaseErrorKind::Timeout`。
    - 错误按协议 `code` 映射为 typed `DatabaseErrorKind`（connection/authentication/
      syntax/not_found/permission/cancelled/constraint/timeout/driver/other），
      不匹配 message 文本。
    - `close` 先发 `close` 等 2s，再关闭 stdin、轮询退出，最后 `kill`；读取线程
      EOF 时把所有挂起请求标记为 connection 错误；`ExtensionProcess::Drop` 同样
      终止子进程，且命令设置 `kill_on_drop(true)`，避免孤儿进程。
    - `execute` 响应解析为列/行/影响行数；句柄的 cancel 通过 stdin 发送 cancel
      帧，行任务观察取消标志后停止发送。
    - 凭据只通过 `connect` 的 JSON 参数发送，要求 UTF-8；配置与错误信息不记录口令。
  - `crates/database_core/src/bin/mock_database_driver.rs`（新）：真实子进程 mock
    驱动，覆盖 connect/test/catalogs/describe_table/execute/begin/commit/rollback/
    explain/cancel/close，支持 `slow` 取消、`hang` 不响应超时与 `--pid-file`。
  - `crates/database_core/tests/extension_driver.rs`（新）：通过
    `CARGO_BIN_EXE_mock-database-driver` 启动真实子进程，端到端覆盖全部方法、错误
    分类、会话取消、超时与 drop 后进程退出（Linux 检查 `/proc/<pid>`）。
  - `crates/database_ui/src/database_ui.rs`：新增
    `register_extension_driver(config, cx)`，扩展驱动与内建 SQLite 走同一个
    `ConnectionManager` 注册表，UI 仍只按 capabilities 决策。
  - `crates/zzz/src/main.rs`：启动时把已安装扩展的 `database_drivers` 解析为
    `ExtensionDriverConfig`，driver id 命名为 `<extension-id>.<manifest-id>`；
    订阅 `ExtensionStore::ExtensionsUpdated` 以便安装/重载后重新注册。清单能力映射
    到 `Capabilities`；协议 v1 的 `execute` 一次返回完整响应，故 `streaming` 固定为
    false，不向 UI 虚报流式能力。
- **验证命令与 exit status**：
  - `cargo check -p zzz` → `EXIT=0`
  - `cargo test -p database_core --test extension_driver` → `EXIT=0`（5 passed）
  - `cargo test -p database_core -p database_ui -p extension -p csv_preview -p i18n`
    → `TEST=0`（core 65 + integration 5、extension 19、ui 18、csv 16、i18n 3）
  - `cargo clippy -p database_core -p database_ui -p extension -p extension_host
--all-targets -- -D warnings` → `CLIPPY=0`
  - `cargo xtask check` → `XCHECK=0`（6 checks）
  - `cargo fmt --all -- --check` → `FMT=0`
  - `cargo test -p extension_host --lib`：
    `test_extension_store_with_test_extension` 首次运行超时（`timed out after 60s
while awaiting install_dev_extension`）。原因是该测试使用真实 HTTP 客户端首次下载
    wasi-sdk：归档 183 MB，本机实测 `curl` 耗时 144s，已远超测试自带的 60s 预算；
    中断留下的 `target/wasi-sdk.tar.gz` 解压报 `Unexpected EOF`，确认是下载未完成而
    非断言失败。把 sha256 匹配的 wasi-sdk（`b761e3a0…`）预置到 `target/wasi-sdk`
    后重跑，`5 passed`。该路径属于 `crates/extension` 的既有行为，本阶段未修改
    `extension_builder`，因此判定为环境限制而非回归。
- **实现决策（记录）**：
  - 协议 v1 以 `id` 关联请求/响应，允许 `cancel` 在 `execute` 等待期间插入；主机
    不按方法名或错误文本做业务分支。
  - 取消存在两条路径：已返回句柄走 `QueryHandle::cancel`，未返回句柄走
    `DriverConnection::interrupt`（连接级），两者最终都让 `execute` 以 `Cancelled`
    终态返回。
  - `execute` 的响应在主机侧仍一次物化后再填充有界 `RowStream`；行窗口上限仍由
    `QuerySession` 保证，但子进程内存不受主机约束。这是 v1 的限制，已把
    `streaming` 上报为 false。
  - 扩展驱动尚未接入连接对话框的驱动选择器；当前可在 `database.connections` 设置中
    写 `driver = "<extension-id>.<driver-id>"`。卸载扩展后其驱动保留在运行时注册表
    直到重启（追加注册，不主动注销）。
- **结论**：PASS。完成定义第 6 条满足：存在真实 subprocess 的 mock 驱动端到端测试；
  能力来自清单并驱动 UI；超时、错误分类、取消与关闭/Drop 清理都有实现与测试。
- **提交 SHA**：见 `git log -1` 于 `database-explorer`
  （`feat(database_core): run extension database drivers as subprocesses`）。
- **下一步**：Phase 7 —— ER 图与导入导出。

## Phase 7 日志 {#phase-7-log}

### ER 图、导出与导入

- **Baseline**：`database-explorer` @ `10756066b8`（Phase 6 提交后）。
- **改动**：
  - `crates/database_core/src/er_diagram.rs`（新）：`er_diagram(&[TableSchema])`
    生成 Mermaid `erDiagram`。实体为 `type name [PK|FK]` 块，外键生成
    `parent ||--o{ child : "columns"` 关系；只在两端都被描述时输出关系，避免悬空实体。
    标识符按 Mermaid 词法归一（保留 Unicode 字母，因而 CJK 可用；空格、点号、引号、
    花括号变成 `_`），归一后重名追加 `_2`。`diagrammable` 过滤空列的表/视图。
  - `crates/database_core/src/export.rs`（新）：把 Phase 4 的导出实现从 `session.rs`
    平移过来并按职责归类；新增 `ExportFormat::Json`（对象数组，保留数值/NULL/布尔类型，
    `Numeric` 仍为字符串以免精度丢失，BLOB 仍是摘要字符串）与
    `export_schema(&[TableSchema], SchemaExportFormat::{Csv, Json})`（CSV 每列一行并标注
    PK/FK，JSON 每表一个对象含列、主键、外键）。
  - `crates/database_core/src/import.rs`（新）：RFC 4180 解析（引号内逗号/换行、双引号
    转义、`\r\n`、UTF-8 BOM、末尾空行）、`prepare_import` 把单元格按列类型转换并校验
    表头（未知列、缺少非空列、类型不匹配、二进制列都给出 typed `ImportError`）、
    `insert_statement` 只生成占位符 `INSERT`。
  - `crates/database_core/src/session.rs`：`import_csv(&TableRef, &str)` 先
    `describe_table` 再解析；在事务内用 `SAVEPOINT zzz_import_batch` 逐行参数化插入，
    成功则把语句/行数计入 `applied_*` 并保持事务打开（由 Commit/Rollback 收尾）；失败
    回滚到 savepoint，若事务是本调用开启的还会整笔回滚。导出代码移出后新增
    `ImportOutcome` 与 `ImportFailure::{Parse, Database}`。
  - `crates/database_core/src/sql.rs`：`quote_identifier` / `quote_table` 成为唯一实现，
    `session.rs` 与 `sqlite.rs` 的重复副本删除（sqlite 的引号测试随之移除，改为由
    `sql.rs` 的用法覆盖）。
  - `crates/database_core/src/model.rs`：`Value::to_json()`（BLOB 摘要、`Json` 列能解析
    则输出 JSON 否则字符串、非有限浮点退回字符串）；`TableRef::qualified_name()`
    统一 `catalog.schema.name` 的文本形式。
  - `crates/database_ui/src/er_diagram.rs`（新）：把当前主题映射为
    `mermaid_render::MermaidTheme`，`render_diagram` 在后台线程把 SVG 栅格化为
    `RenderImage`；`ErDiagram` 保存 source 与渲染结果。
  - `crates/database_ui/src/result_set_view.rs`：
    - schema 树顶部新增 `Schema CSV` / `Schema JSON`（复制到剪贴板），展开表时缓存
      `TableSchema`，`describe_all_tables` 只补描述缺失的表，ER 图与 schema 导出共用。
    - 工具栏新增 `ER Diagram`（仅 `capabilities.er_diagram`），面板显示渲染图，可
      `Copy Mermaid` 或 `Back to Query`；无能力/无表/渲染失败都有明确文案。
    - 表详情新增 `Import CSV…`（`row_editing` + `transactions` + 表而非视图时），
      走平台文件选择器，读完文件后调用 `import_csv`。
    - 结果集复制按钮新增 `Copy JSON`；非 SELECT 语句成功后清空 schema 缓存与 ER 图，
      避免 DDL 后展示过期结构。
    - 编辑栏不再要求已有结果集（导入会先开事务），因此导入后可直接 Commit/Rollback。
  - i18n：新增 25 个 `database.er.*` / `database.import.*` / `database.schema.*` /
    `database.grid.copy_json` 键；en/zh 各 3979 条且 key 集合一致，代码 fallback 与
    `en.json` 完全一致。
- **验证命令与 exit status**：
  - `cargo test -p database_core -p database_ui --all-targets` → `TEST=0`
    （core 84 + integration 5、ui 20）
  - `cargo clippy -p database_core -p database_ui --all-targets -- -D warnings`
    → `CLIPPY=0`
  - `cargo check -p zzz` → `ZCHECK=0`
  - `cargo xtask check` → `XCHECK=0`（6 checks）
  - `cargo fmt --all -- --check` → `FMT=0`
  - `cd docs && npx prettier --check src/` → PASS
  - i18n fallback 脚本比对 `crates/database_ui/src` 的 119 处 `tr(...)` → 0 mismatch
- **新增测试**：
  - `er_diagram`：表/列/PK/FK 输出、CJK 表名与列名的 PK/FK 标记与自引用关系、标点归一
    与重名后缀、未描述目标的 FK 被省略、schema 限定名保留在实体名中。
  - `database_ui::er_diagram`：CJK schema 生成的源码交给真实 Mermaid 解析器渲染成功
    （`render_to_svg` 返回 SVG），并覆盖无外键的情形。
  - `export`：JSON 保留类型与 NULL、按显示顺序导出、schema CSV 的列/键标注、schema
    JSON 的主外键结构；`session` 保留原有的「导出使用暂存值」测试。
  - `import`：引号/BOM/内嵌换行、参差行报错、未闭合引号、空文档、按表列顺序绑定、
    未知列/缺列/类型错误、可空列可缺省、`INSERT` 仅含占位符。
  - `session::import_csv`（文件数据库双连接）：导入后第二连接读不到，Commit 后可见；
    Rollback 后行消失；批次中途违反 UNIQUE 时整笔不留且本调用开启的事务关闭；
    未知列返回 `ImportFailure::Parse` 且不开启事务。
- **实现决策（记录）**：
  - 导出走剪贴板（结果集 CSV/JSON/Markdown/HTML，schema CSV/JSON），不新增保存对话框：
    计划验收只要求内容正确，文件对话框在无显示环境下无法验证，留到用户文档阶段再定。
  - 导入的「影响行数」按成功执行的 `INSERT` 条数计：SQLite 在 step 之前就返回元数据，
    驱动无法在此路径报告 `sqlite3_changes`。
  - 导入要求 `transactions && row_editing`，因为回滚能力是验收条件；只读驱动直接拒绝
    而不降级为自动提交。
  - ER 图按需描述所有表并跳过描述失败的表（视图或权限不足不应让整张图失败）；
    schema 缓存以表引用为键，DDL 成功后整体失效。
- **结论**：PASS。CJK 表名经真实 Mermaid 渲染器验证；导入在事务内且可回滚（含批次
  中途失败）；结果集与 schema 都有 CSV/JSON 导出。
- **提交 SHA**：见 `git log -1` 于 `database-explorer`
  （`feat(database_ui): diagram schemas and import csv in transactions`）。
- **下一步**：Phase 8 —— Visual EXPLAIN。

## Phase 8 日志 {#phase-8-log}

### Visual EXPLAIN

- **Baseline**：`database-explorer` @ `d90e2ff59c`（Phase 7 提交后）。
- **改动**：
  - `crates/database_core/src/explain.rs`（新）：`flatten_plan(&ExplainPlan)` 把驱动的
    扁平 `(id, parent, detail)` 行重建为深度优先顺序，父节点总在子节点之前。父 id
    不存在、`parent == id`、`parent == 0` 都按根处理；环只输出一次，未到达的节点在末尾
    以深度 0 补上，因此畸形计划既不会丢失也不会无限递归。
  - `crates/database_core/src/session.rs`：新增 `QuerySession::explain(sql)`，先检查
    `capabilities.explain` 再委托驱动，避免调用方拿到驱动特定错误。
  - `crates/database_core/src/sqlite.rs`：已有 `EXPLAIN QUERY PLAN` 解析保持不变，新增
    两项真实计划测试（主键查找必须是 `SEARCH` 而非全表扫描；JOIN + ORDER BY 产生多节点
    且包含两张表名）。
  - `crates/database_ui/src/result_set_view.rs`：工具栏新增 `Explain`（仅
    `capabilities.explain` 时出现，因此无该能力的驱动根本不显示动作）；计划以缩进树
    展示（每行显示驱动节点 id 与 detail），可 `Back to Query`。`show_explain` 与
    `show_er_diagram` 互斥，运行新查询或打开 ER 图都会退出计划视图；空计划给出
    「驱动未返回计划」提示而不是空白面板。
- **验证命令与 exit status**：
  - `cargo test -p database_core -p database_ui --all-targets` → `TEST=0`
    （core 91 + integration 5、ui 20）
  - `cargo clippy -p database_core -p database_ui --all-targets -- -D warnings`
    → `CLIPPY=0`
  - `cargo check -p zzz` → `ZCHECK=0`
  - `cargo xtask check` → `XCHECK=0`
  - `cargo fmt --all -- --check` → `FMT=0`
  - `cd docs && npx prettier --check src/` → PASS
  - i18n fallback 脚本比对 126 处 `tr(...)` → 0 mismatch；en/zh 各 3983 条且 key 一致
- **新增测试**：计划嵌套顺序、父 id 缺失按根、环只输出一次且深度正确、空计划；
  SQLite 主键查找报告 `SEARCH`、JOIN 计划多节点并包含表名；能力缺失时
  `QuerySession::explain` 返回 `DatabaseErrorKind::Driver`，有能力时返回节点。
- **实现决策（记录）**：
  - 计划视图复用结果区而不是新增标签页：它描述的是「当前语句」，与结果集互斥。
  - 计划文本保持驱动原文，只做缩进与 id 标注；不做语句重写或成本估算，因为不同方言的
    计划字段含义不同，D-01 要求能力而非方言分支。
- **结论**：PASS。典型查询计划可读；能力缺失时动作不出现。
- **提交 SHA**：见 `git log -1` 于 `database-explorer`
  （`feat(database_ui): show the query plan as a tree`）。
- **下一步**：Phase 9 —— 查询历史。

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
