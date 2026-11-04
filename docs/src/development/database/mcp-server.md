# 数据库 MCP server {#database-mcp-server}

`database-mcp-server` 把已保存的 SQLite 连接以 MCP tools 暴露给 agent：列出连接、描述表、
执行只读查询。它遵守 [D-03](./decisions.md#d-03)：MCP 只是把只读查询能力交给外部 AI
代理，不承担驱动加载、连接生命周期或凭据管理。

## 定位 {#purpose}

- 复用 `database.connections`（与浏览器相同的设置），不新增连接配置。
- 只打开文件型 SQLite；其他驱动的连接会出现在 `list_connections` 里并标记
  `available: false`，而不是在调用时失败。
- 不读取 keychain：文件型 SQLite 不需要口令，设置文件里也从不存口令（D-09）。

## 构建与启动 {#build}

```sh
cargo build -p database_core --bin database-mcp-server
# 产物：target/debug/database-mcp-server
```

在 ZZZ 的 `settings.json` 里登记为本地 MCP server：

```json
{
  "context_servers": {
    "database": {
      "command": "/absolute/path/to/database-mcp-server",
      "args": []
    }
  }
}
```

参数：

| 参数              | 含义                                                           |
| ----------------- | -------------------------------------------------------------- |
| `--settings PATH` | 读取指定设置文件，默认是 `paths::config_dir()/settings.json`   |
| `--allow-writes`  | 额外暴露 `run_write_query`；不加时该工具不存在，调用也会被拒绝 |

## Tools {#tools}

### `list_connections`

无参数。返回 `connections` 数组，每项是：

```json
{ "id": "work", "name": "Work", "driver": "sqlite", "available": true }
```

不可用的连接额外带 `reason`。不返回路径、主机、用户名或任何设置中的其他字段。

### `describe_table`

参数 `connection`（id）与 `table`（可用 `schema.table` 或 `catalog.schema.table`）。
返回列名、声明类型、可空性、主键列与外键；表不存在时返回 tool error，不会给出空 schema。

### `run_read_query`

参数 `connection`、`sql`、可选 `limit`（默认 500，上限 5000）。只接受**单条**
`SELECT`、`VALUES`、`TABLE` 或 `EXPLAIN` 语句。返回：

```json
{
  "columns": [{ "name": "id", "type": "Integer" }],
  "rows": [[1], [2]],
  "truncated": false
}
```

- `NULL` 是 JSON `null`。
- BLOB 不内联，返回摘要字符串，例如 `[blob 3B: 00ff10]`（D-07）。
- 达到 `limit` 且仍有行时 `truncated` 为 `true`，服务端会取消剩余的流式读取。

### `run_write_query`（仅 `--allow-writes`）

参数 `connection`、`sql`。接受单条数据或 schema 语句，拒绝空语句与 `BEGIN`/`COMMIT`/
`ROLLBACK`/`SAVEPOINT` 等事务控制语句。返回 `{ "affected_rows": N }`。

## 只读保证 {#read-only}

两层保护，缺一不可：

1. `database_core::sql::is_read_only` 在语句级拒绝写语句。它是防误用的分类器，不是完整
   SQL 解析器；`WITH ... SELECT` 允许，`WITH ... DELETE` 拒绝。
2. 执行前先在同一条连接上 `PRAGMA query_only = 1`。即使分类器误判，SQLite 本身也不会
   写入。

连接在每次 tool 调用时打开，调用结束即关闭，因此 `query_only` 不会泄漏到其他调用，也不会
影响 ZZZ 浏览器里的连接。

## 安全边界 {#security}

- tool 输出只包含：连接 id/名称/驱动、表结构、查询结果、错误文本。设置文件中的
  `password`、`token` 等字段不会被反序列化进响应结构（`SavedConnection` 只声明它用的
  字段），测试用逐行断言验证它们不出现在 stdout。
- 错误文本来自驱动，可能包含表名或 SQLite 的诊断信息，但不含凭据。
- 写模式必须显式加 `--allow-writes`；这条路径没有事务包裹，适合单条维护语句，不适合
  多语句迁移。

## 已知限制 {#limitations}

- 只支持 `database_core` 内建的 SQLite 驱动。扩展驱动需要单独提供 server，或等协议扩展。
- 每个请求同步执行（`smol::block_on`），同一 server 进程一次处理一条 tool 调用；并发由
  客户端启动多个进程或串行调用解决。
- 服务读取的是本机设置文件与本地 SQLite 文件；远程项目文件不在 Phase 12 的范围内
  （见 [remote-runbook](./remote-runbook.md)）。
- 发行包不会自动登记这个 MCP server；需要使用者把 `context_servers` 指向构建出的二进制。

## 验证 {#verification}

真实 subprocess 测试在 `crates/database_core/tests/mcp_server.rs`：启动真正的
`database-mcp-server`，覆盖 `initialize`、`tools/list`、`list_connections`、
`describe_table`、BLOB/NULL 结果、拒绝写语句、设置中的假口令不出现在输出，以及
`--allow-writes` 下的写入与回读。

```sh
cargo test -p database_core --test mcp_server
```
