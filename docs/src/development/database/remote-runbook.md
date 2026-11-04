---
title: Database Remote Runbook
description: Evidence, steps, and risks for remote SQLite access and SSH tunnels.
---

# 远程与 SSH：实施 runbook（Phase 11，NOT RUN） {#remote-runbook}

本文件是 Phase 11 的结论与可执行 runbook。当前结论是 **拒绝在本环境实施，验证状态
`NOT RUN`**，不是 PASS。原因与证据见下文；在具备远程项目环境的机器上，按第 3 节步骤
实施并逐项勾选第 4 节。

## 1. 结论与范围 {#conclusion}

Phase 11 包含两件独立的事：

1. **远程项目中的 SQLite 文件**：项目通过 `remote_connection` 挂在远端，数据库文件只
   存在于远端文件系统；本地应用不能直接 `open(2)` 它。
2. **SSH 隧道**：`DataSource::Server { host, port }` 需要把远端数据库端口转发到本地，
   或反向地通过远程项目的 `RemoteClient` 建立转发。

本阶段**未实现**任何一项，理由：

- 此仓库的数据库测试没有任何远程项目夹具。`crates/database_core` 与
  `crates/database_ui` 的测试全部在本地文件或内存 SQLite 上运行；无法在 clean
  baseline 下复现一个可验证的远程环境，因此按计划要求标记 `NOT RUN`，不冒充 PASS。
- 内建驱动只有 SQLite（文件型）。没有网络驱动时，SSH 隧道没有可实施的服务端目标；
  先写一条无法被任何测试或真实连接使用的隧道代码属于死代码。
- 直接用 `crates/fs` 把远端 SQLite 文件读成本地临时副本再打开，会在写入、锁和
  WAL 上产生静默分歧（本地提交不会回到远端，远端并发写会覆盖本地副本）。这不满足
  编辑与事务语义（Phase 5 的验收），因此拒绝这条捷径。

## 2. 现状证据 {#evidence}

- `crates/database_core/src/sqlite.rs:112`：`DataSource::File { path }` 直接进入
  `path_to_sqlite_uri`（同文件 `:160`），由 `libsqlite3-sys` 在**本进程**打开。没有
  经过 `crates/fs`、`crates/remote_connection` 或任何 RPC。
- `crates/database_core/src/model.rs:660-664`：`DataSource` 只有
  `File { path: PathBuf }`、`Server { host, port }`、`InMemory { name }`；`PathBuf`
  不携带「这是哪个项目、是本地还是远端」的来源信息，因此驱动无法判断一个路径是否
  属于远程工作区。
- `crates/database_ui/src/connection_dialog.rs:208-219`：连接对话框允许直接输入
  `path`，并把它原样变成 `DataSource::File`，从不查询 `Project`。若用户输入远程项目
  的路径，本地驱动会打开同名本地文件或报错，而不是访问远端。
- `crates/repl/src/kernels/ssh_kernel.rs:116-133`：
  `RemoteClient::build_forward_ports_command(forwards)` 生成 `ssh` 命令并用
  `util::command::new_command` 启动，`_ssh_tunnel_process` 由 kernel 持有，drop 时
  `kill`（同文件 `:345`）。这是 D-10 指定复用的隧道机制。
- `crates/project/src/project.rs:2547`：`Project::is_via_remote_server()` 可判断当前
  项目是否经由远程服务器；这是把「本地/远程」信息带入数据库连接所需的最小入口。
- `grep -rn "remote_connection\|ssh_kernel" crates/database_core/src
crates/database_ui/src` 无匹配：数据库代码目前与远程路径完全没有接线。

## 3. 实施步骤 {#steps}

以下步骤按依赖顺序排列。

### 3.1 为数据源记录来源 {#provenance}

1. 在 `ConnectionConfig` 中增加非敏感来源字段（D-09 允许，不存口令），例如
   `project_id: Option<u64>` 与 `project_path: Option<ProjectPath>`，或复用
   `Project::remote_id()` 的返回值。
2. 更新 `crates/database_ui/src/connection_settings.rs` 的
   `connection_from_content` / `connection_to_content` 与 `settings` 结构；旧设置缺少
   该字段时按本地处理（向后兼容）。
3. 连接对话框在创建 `DataSource::File` 前，若来源项目 `is_via_remote_server()`，
   要么走 3.2 的远程路径，要么明确拒绝并提示「远程文件暂不支持」，不得静默当成本地
   文件。

### 3.2 远程文件：在远端执行驱动 {#remote-driver}

推荐方案：把 `DatabaseDriver` 放到远端进程执行，而不是搬文件。

1. 在 `crates/rpc` 增加数据库请求/响应（复用现有 RPC 传输与序列化）。请求至少包含
   连接配置（不含口令）与 SQL；响应包含 `QueryMetadata`、行流或完整行集、错误分类。
2. 复用 `crates/remote_connection` 的远程客户端通道；在远端服务器侧注册一个
   `DatabaseDriver` 服务，进程内仍使用 `crates/database_core::SqliteDriver` 打开
   **远端本地路径**。
3. 取消走同一通道发送 cancel；连接断开时远端必须取消该连接的所有查询（复用
   `ConnectionManager` 的既有语义）。
4. 本地侧新增代理驱动（实现 `DatabaseDriver` / `DriverConnection` / `QueryHandle`），
   让 `database_ui` 与 `database_core` 的其余代码完全不用知道连接是不是远程的。

备选（仅只读）：如果只需要浏览，可用 `Project::fs` 的 `Fs` trait 读文件到
`paths::temp_dir()`，打开后标记为只读；必须显式提示「远程副本，不写回」，不得把它当成
普通可编辑连接。

### 3.3 SSH 隧道（`DataSource::Server`） {#tunnel}

1. 在连接打开路径上，若 `DataSource::Server` 且存在远程项目上下文：
   - 用 `RemoteClient::build_forward_ports_command` 转发一个本地临时端口到
     `host:port`，命令与环境变量照抄 `ssh_kernel` 的启动方式。
   - 等待隧道就绪（参考 `ssh_kernel.rs:160-200` 的重试与日志）。
2. 把转发后的 `127.0.0.1:<临时端口>` 交给驱动，原始 `host/port` 不再直接使用。
3. 隧道的 owner 必须是 `ManagedConnection`：`disconnect` 先关闭隧道进程再关闭驱动；
   `reset`/drop 时 `kill`。禁止让 `ssh` 子进程脱离连接生命周期。
4. 本地项目 + 用户自己配置的 SSH 主机：另开一个「SSH 隧道」设置（主机、端口、用户），
   仍复用同一 `build_forward_ports_command` 之外的最小 `ssh -L` 生成逻辑；不要把
   `repl` 的 kernel 生命周期搬进数据库层。

### 3.4 明确不做 {#not-doing}

- 不把远端 SQLite 文件复制到本地后允许写入。
- 不新增第二套远程传输；只走 `remote_connection` / `crates/fs` 已有路径。
- 不引入默认远程主机或云端数据库托管（D-12）。

## 4. 验证清单（在具备远程环境的机器上执行） {#verification}

前提：一台可 SSH 的远端主机，远端安装 ZZZ server；一个包含 `.sqlite` 文件的远程项目。

| #   | 操作                                 | 期望                                             |
| --- | ------------------------------------ | ------------------------------------------------ |
| 1   | 在远程项目中打开该 SQLite 文件       | schema 树来自远端实查；不读取本地同名文件        |
| 2   | 执行 `SELECT`                        | 结果正确；`EXPLAIN`/历史/Notebook 照常工作       |
| 3   | 在远端用另一进程写入一行，再刷新     | 能看到远端的新数据（证明不是本地副本）           |
| 4   | 应用内提交一次编辑                   | 远端文件变化；本地无可疑副本                     |
| 5   | 取消长查询                           | 只取消该查询；连接仍可继续查询                   |
| 6   | 断开远端连接                         | 隧道进程与查询都被关闭，`ps` 无残留 `ssh` 子进程 |
| 7   | 配置 `DataSource::Server` + SSH 隧道 | 通过 `127.0.0.1:<临时端口>` 连通；端口不直接暴露 |
| 8   | 隧道建立失败                         | 连接报分类错误，不 fallback 到直连 `host:port`   |

自动化建议：

```sh
cargo test -p database_core -p database_ui
cargo test -p remote_connection
cargo test -p repl ssh_kernel
```

远程夹具至少需要：一个本地回环 SSH 服务器（CI 可用 `sshd` 容器）或仓库既有的远程项目
测试支持；两者在当前环境都不可用，故本文件状态为 `NOT RUN`。

## 5. 风险 {#risks}

- **静默读到错误文件**：远程路径与本地路径同形。未接入 3.1 前，UI 必须避免声称支持
  远程文件。
- **事务语义**：复制方案会破坏事务、锁与 WAL；这也是拒绝它的原因。
- **凭据**：远程 RPC 只传连接配置，口令仍由本地 keychain 在建立连接时注入；不要把
  口令放进 RPC 日志或 tool 输出（D-09）。
- **进程孤儿**：隧道进程必须由 `ManagedConnection` 持有；参考 `ssh_kernel` 的
  `_ssh_tunnel_process` 与 `Drop` 行为。
