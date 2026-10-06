---
title: VFS Architecture Decisions
description: Frozen implementation decisions for the ZZZ provider and snapshot VFS.
---

# VFS 架构决策

本文冻结 Phase 1 公共类型依赖的架构决策。状态为 `ACCEPTED` 的决策只能通过新的
ADR 修改，不能在实现中隐式漂移。

- 状态：`ACCEPTED`
- 决策日期：2026-10-05
- 实现基线：`a3a0f9734069b543f3fe1e0bdd77a37fbd1b2b31`
- 架构来源：[VFS Architecture Research](./report.md)
- 执行合同：[VFS Refactoring Execution Plan](./refactoring-plan.md)

## 决策摘要 {#decision-summary}

| 主题                | 决策                                                                            |
| ------------------- | ------------------------------------------------------------------------------- |
| Path encoding       | component-based `bytes + PathEncoding`，identity 与 display 分离                |
| Resource identity   | `ResourceId` 只保证 session 内稳定，持久化使用 mount descriptor 与 exact path   |
| Provider capability | 结构化 capability、limit 与 guarantee，不通过试错探测                           |
| File I/O            | `read_at`/`write_at` 为基础 primitive，cursor 是上层 adapter                    |
| Snapshot            | 惰性树、版本状态、显式容量与 eviction，watch event 只作 hint                    |
| Watch recovery      | 每个 subscription 单调 sequence；缺失历史必须 `Overflow` 并 rescan              |
| Remote retry        | 读操作可安全重试；mutation 依赖 operation ID、idempotency key 与 result journal |
| Containment         | mount-relative validation 加 execution-host containment；默认不跟随 symlink     |
| Writes              | `expected_version` 与明确 atomicity；未知提交状态不能盲目重放                   |
| LSP/native          | `LspPathMapper` 与 `NativeExecutionContext` 是显式 capability boundary          |
| Archive             | data-local、只读、range-based，并执行固定资源与安全上限                         |
| Composition         | Subtree/ReadOnly/Cache 必做；当前 overlay 因缺少 crash-safe transaction 被拒绝  |

## Exact path wire format {#exact-path-wire-format}

VFS wire path 按 component 编码，不使用 separator-delimited string。逻辑 schema 为：

```protobuf
message ExactPath {
  PathEncoding encoding = 1;
  PathRoot root = 2;
  repeated bytes components = 3;
}
```

`PathEncoding` 的语义固定为：

- `UNIX_BYTES`：POSIX 原始文件名 bytes。
- `WINDOWS_WTF8`：从 Windows `OsString` 的 WTF-16 无损转换得到的 WTF-8 bytes，允许
  lone surrogate。
- `PORTABLE_UTF8`：经 UTF-8 校验、适用于生成资源和跨平台逻辑名称的 bytes。

`VfsPath` 与 `ProviderPath` 永远 mount-relative，因此 root 必须是 `RELATIVE`。
`NativePath` 才允许 POSIX root、Windows drive、UNC、verbatim drive 和 verbatim UNC。
Root 的 drive、server 与 share 也使用 exact bytes，不进入 display string。

每个 component 必须满足：

1. 非空；
2. 不含 NUL；
3. 不是 `.` 或 `..`；
4. 不含该 encoding 的 separator；
5. join 后不能替换 root 或越过 mount root。

Operational equality 使用 provider 返回的 exact component 与 `lookup_key`。Display
转换可以 lossy，但 display 结果不得参与 hash、lookup、persistence、RPC 或权限判断。
URI 是 LSP、plugin 和 persistence interoperability 表示，不是 VFS primary key。

固定 corpus 位于 `crates/vfs/test_data/path_corpus.json`，schema version 为 `1`，随机
seed 为 `1592614637`。它包含 POSIX non-UTF-8、Windows drive/UNC/verbatim、lone
surrogate、archive raw name 与 invalid traversal/component cases。

## Resource identity lifetime {#resource-identity-lifetime}

`ResourceId` 由 `MountId + node_id + generation` 组成。

- `MountId` 在一个 `VfsManager` session 内唯一。
- `node_id` 由 snapshot registry 单调分配。
- registry 重用 node slot 时必须增加 `generation`。
- provider stable file key 可以在 rename 后保持 `ResourceId`，但不能跨越 mount
  replacement 或无法证明的 delete/create。
- workspace/session persistence 不保存 runtime `ResourceId`。它保存版本化 mount
  descriptor、exact `VfsPath` 和可选 provider stable key hint。
- reconnect 可以恢复同一 mount session；进程重启后重新 intern，并向 consumer 发布
  mapping change。

## Provider capability model {#provider-capability-model}

Capability 使用结构体而不是单一 bitflag。它至少包含：

- supported operations；
- read/write/append/truncate/positioned I/O；
- same-mount rename/copy；
- conditional mutation 与 idempotency；
- watch、recursive watch 与 resumable journal；
- symlink、permissions、xattrs、trash 和 native mapping；
- case behavior；
- atomicity guarantee；
- page、batch、range、request 和 open-handle limits。

Wrapper provider 必须重新计算 capability。`ReadOnlyProvider` 不能透传 write；
`SubtreeProvider` 不能扩大 root authority；`CacheProvider` 不能把 stale data 报成 fresh。
Capability 只表达实现上限，每次 operation 仍必须通过 authorization。

初始远程协议限制为：

- default directory page：1,024 entries；negotiated maximum：4,096；
- `stat_many` maximum：4,096 resources；
- positioned read/write chunk maximum：1 MiB；
- 单 protobuf control message 解码上限：8 MiB；
- 超过限制的数据使用 bounded stream 与 backpressure，不能收集成单个 `Vec<u8>`。

## Positioned I/O and cancellation {#positioned-io-and-cancellation}

`VfsFile` 的基础接口为 `len`、`read_at`、`write_at`、`set_len`、`flush` 与 `sync`。
并发 positioned I/O 不共享 cursor。

- Unix local provider 使用 `FileExt::read_at`/`write_at`。
- Windows local provider 使用 `seek_read`/`seek_write`。
- Remote provider 把 offset、bounded length 与 operation ID 放入 RPC。
- Decoder 需要 `AsyncRead + AsyncSeek` 时使用单独 cursor adapter；adapter 自己串行化
  cursor，不改变底层 file contract。
- Dropping request future 表示 cancellation。Remote operation 还发送显式 cancel，以便
  服务端及时释放 handle、buffer 与 mutation staging state。

EOF 返回本次实际读取 byte count。超出 EOF 的 read 返回 `0`，不是错误。Sparse file
支持由 capability 表达；不支持的 provider 返回 typed `Unsupported`。

## Snapshot freshness and budgets {#snapshot-freshness-and-budgets}

Snapshot entry 状态为 `Loading`、`Fresh`、`Stale` 或 `Tombstone`。Metadata、directory
children 与 content/range cache 分开版本化。读取 stale state 必须触发 refresh 或返回
显式 freshness，不得静默伪装为 fresh。

Phase 0 在当前仓库测得：5,221 files、1,158 directories 的 existing Worktree scan 为
72.26 ms；独立 benchmark process 最大 RSS 为 18,292 KiB。该数值只作为小型本地
baseline，不代表一百万条目验收结果。

`VFS-EXP-006` 固定以下预算：

- provider corpus：1,000,000 entries；固定 seed `1592614637`；
- 默认只展开 root 加 10,000 entries，未展开 descendants 不进入 snapshot node map；
- snapshot-owned heap delta 不超过 128 MiB；
- metadata cache 48 MiB、directory cache 80 MiB、range cache 128 MiB；三者分别 LRU
  eviction，总默认上限 256 MiB；
- cached lookup p95 小于 2 ms；
- root first-paint 小于 250 ms；
- benchmark 必须分别记录 allocator heap delta 与 process RSS，不能把编译器 RSS 算入。

## Watch sequence and reconciliation {#watch-sequence-and-reconciliation}

Provider 在 initial enumeration 前注册 watch。每个 watch subscription 具有独立、单调
递增的 `u64` sequence。

- Event batch 覆盖 `[first_sequence, last_sequence]`，不能存在未声明空洞。
- Duplicate batch 通过 sequence 去重。
- `resume_after_sequence` 只在 provider journal 仍含连续历史时成功。
- journal truncation、backend overflow、sequence gap 或无法证明的 reconnect 都产生
  typed `WatchOverflow`。
- Snapshot 标记最小受影响 root stale，执行 scoped rescan，计算 deterministic diff，
  然后一次发布一致 snapshot。
- Rename 只有 provider stable key 或 reconciliation 能证明时保持 identity；否则是
  delete/create。

## Remote retry and idempotency {#remote-retry-and-idempotency}

每个 remote request 有 connection-local request ID。每个 mutation 另有稳定 operation ID
与 idempotency key。

- `stat`、`read_dir` 和 positioned read 在没有向 caller 发布 partial result时可以重试。
- Stream 已发布数据后断线，caller 获得 `Disconnected` 与最后确认 cursor，自行决定
  resume。
- Mutation server 在执行前登记 operation ID，在 commit 后保存 result journal。
- response 丢失时，client 先查询 operation result。只有 server 明确返回 `NotStarted`
  才允许重放。
- result journal 默认保留 15 分钟或 100,000 operations，以先达到者触发 eviction；
  eviction 后未知 operation 返回 `IndeterminateCommit`，不能报告成功或自动重放。
- reconnect 恢复 mount descriptor、capability、open-handle lease 和 watch cursor。无法
  恢复的 handle 明确失效，watch 进入 overflow/rescan。

Phase 0 fault harness 在 `vfs::test_support::FaultInjectingTransport`。它按 direction 与
sequence 注入 delay、fragmentation、drop 和 disconnect，并保留 reconnect 后的全局
sequence，供 Phase 4 loopback transport 使用。

## Symlink and containment {#symlink-and-containment}

所有 client path 在服务端重新解析和验证。`SubtreeProvider` 先验证 lexical component，
再在 execution host 验证 resolved target 未越过 capability root。

- 默认 stat/list 不跟随最后一个 symlink。
- Follow operation 必须显式请求并受 capability 与 authorization 约束。
- Symlink cycle 返回 typed error。
- Archive symlink 永不自动跟随，也不 materialize 到磁盘。
- `cap-std` 只有在 Local/Subtree provider conformance 和平台 tests 通过后才可作为内部
  containment implementation，不能成为公开 path identity。

## Atomicity and expected version {#atomicity-and-expected-version}

Metadata version 是 provider opaque bytes。Timestamp 只用于显示和兼容，不是并发控制
token。

Write、rename 与 delete 接受可选 `expected_version`。Mismatch 返回 `StaleVersion`，
不得覆盖新内容。Operation options 同时声明需要的 atomicity：

- `BestEffort`
- `AtomicWithinFile`
- `AtomicWithinDirectory`
- `AtomicWithinMount`

Provider 不能保证 caller 请求的级别时返回 `UnsupportedAtomicity`。Cross-provider move
永不宣称 atomic；它先 streaming copy、校验 length/version/content digest，再删除 source。
失败返回精确 partial outcome。

## LSP and native execution mapping {#lsp-and-native-mapping}

`NativeExecutionContext` 只由 resource 所在 host 提供，包含 native path mapping、process、
terminal、Git 与 debugger capability。客户端不能把 remote path 转成本地 `PathBuf`。

`LspPathMapper` 负责：

- `VfsPath` 到 execution-host `file:` URI；
- LSP URI 到 `ResourceId`/`VfsPath`；
- workspace edit 的完整预验证；
- read-only 或 unmappable resource 的 typed rejection。

Archive、memory 和其它无 native mapping 的 resource 不注册为普通 LSP document。
Temporary mirror 默认关闭；若未来启用，必须另行定义 version、cleanup、privacy 与
reverse-edit policy。

## Archive limits {#archive-limits}

`ArchiveProvider` data-local 执行，只读 Central Directory 与实际打开 member 的 range。
默认限制为：

- entry count：250,000；
- single name：16 KiB exact bytes；
- Central Directory：256 MiB；
- single uncompressed member：4 GiB；
- total uncompressed bytes：16 GiB；
- compression ratio：1,000:1；
- nested archive depth：4；
- 单次 index/read CPU wall deadline：30 秒，可取消；
- encrypted、unsupported compression、CRC mismatch、duplicate collision 和 traversal
  分别返回 typed error。

Duplicate exact names 保留原始 record，但 provider lookup 返回 `NameCollision`，不能静默
选择第一个或最后一个。Archive version 由 outer resource version 与 provider format
version 共同组成；外层变化使 child mount stale。

## Composition wrappers and overlay decision {#composition-and-overlay}

`SubtreeProvider` 把 wrapper root 精确映射到 inner prefix，并固定 prefix 的 stable file
key。每次 operation 和已打开 handle 的 I/O 都重新验证 root identity 与路径 component；
wrapper 不跟随 symlink，也不暴露 host-native path。Root replacement 返回
`StaleVersion`，`..`、absolute replacement 和 prefix 外 entry 不进入公开 identity。

`ReadOnlyProvider` 保留 read/list/watch 能力，但把全部 write、mutation、permissions、trash
和 native-path capability 降为 `Unsupported`。任何 provider mutation 或 `VfsFile`
write/set-length/flush/sync 返回 typed `ReadOnly`。

`CacheProvider` 的 range key 固定为
`(mount, exact path, offset, length, source version)`。默认 range LRU 上限为 128 MiB 和
4,096 entries；cache hit 与 inner read 前后都复核 source version，读取期间发生版本变化时
返回 `StaleVersion`，不会把新字节挂到旧版本 key。任何可能部分成功的 mutation 都清空
cache。

Overlay specification 冻结为：upper 优先；directory listing 合并 upper/lower；durable
whiteout 隐藏 lower entry；lower-only mutation 必须 copy-up；跨层 rename 必须把 target
publish 与 source whiteout 作为一个 crash-consistent commit；任一 source disconnect 转成
rooted `Overflow` 并重新建立 watcher ordering。

当前 `VfsProvider` 没有 durable whiteout primitive 或 atomic multi-path commit。Model test
证明 lower-only rename 在 copy-up 后、whiteout 前崩溃会同时暴露 source 与 target。因此
Phase 8 的 `OverlayProvider` 决策为 `REJECTED`，不保留 prototype。只有新的 transaction
contract 能同时证明 whiteout durability、cross-layer rename atomicity 和 watcher recovery
时才重新评估。

## Compatibility policy {#compatibility-policy}

旧 `RelPath`、`ProjectPath` 和 protobuf string path adapter 只允许返回可表示的 UTF-8
路径。遇到不可表示名称返回 typed compatibility error。Adapter 必须记录 caller 与删除
Phase，禁止 fallback 到 `to_string_lossy()`。

OpenDAL、`rust-vfs` 与 Wasmer `virtual-fs` 不进入公开 API。`typed-path` 可以作为 lexical
parser，`notify`、`cap-std` 和 `async_zip` 只能位于 provider 内部。
