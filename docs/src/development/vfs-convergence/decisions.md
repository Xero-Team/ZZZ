---
title: VFS Convergence Decisions
description: Frozen technical decisions for the VFS convergence and extension work.
---

# VFS 收敛冻结决策

本文记录实现计划中不可在执行期临时更改的技术决策，以及被拒绝的替代方案。每条决策
包含：结论、理由、替代方案为何被拒绝、何时可以重新讨论。

前序决策见[VFS Architecture Decisions](../vfs-research/adr.md)。本文不推翻任何一条
前序 ADR；如有冲突以本文为准，并在账本记录冲突原因。

## D1：WatchSupervisor 属于 `crates/vfs` {#d1-watch-supervisor}

**结论**：supervisor 实现为 `crates/vfs/src/watch_supervisor.rs`，对外暴露
`VfsSnapshot::watch(WatchRequest) -> WatchSubscription`；worktree 只消费
`SnapshotDelta`，不实现自己的事件循环。

**理由**：

1. watch 语义（sequence 连续性、overflow 重建、断线恢复）是 provider 契约的一部分，
   放在 worktree 会让每个新 consumer 重新实现一遍。
2. `VfsSnapshot::apply_event_batch` 已经存在（`crates/vfs/src/snapshot.rs:776`），
   supervisor 只是它的驱动者，不会引入新的抽象层。
3. `crates/vfs` 不依赖 settings 与 GPUI，可以独立测试。

**拒绝的替代方案**：

- 在 worktree 内做 supervisor：会把 reconciliation 与 UI 刷新耦合，且 archive 与
  terminal_mode 无法复用。
- 让 provider 自己推送已 reconcile 的 delta：越过 snapshot，破坏“snapshot 是真相”。
- 保留 `fs.watch` 为主路径、VFS watch 为可选：即现状，两套引擎并存，视为缺陷。

**重新讨论条件**：出现第三类 consumer 需要与 snapshot 无关的裸事件流。

## D2：sequence 按事件分配，缺口即重建 {#d2-sequence}

**结论**：`EventBatch` 的 `first_sequence`/`last_sequence` 表达真实区间（当前实现
一个批次共用一个 sequence）；supervisor 检测 `first_sequence > last_seen + 1` 时视为
丢事件，按 `Overflow` 处理。`resume_after_sequence` 只在
`WatchCapabilities.resumable_journal == Native` 时传递。

**理由**：

1. 当前本地 provider 对任何 `resume` 都先发 `Overflow`
   （`crates/fs/src/vfs_provider.rs:733-745`），等于假装可恢复。
2. 批次粒度 sequence 无法定位单个事件，缺口检测退化为批次级，掩盖丢事件。
3. 显式声明“不可恢复”比隐式全量重建更容易解释给 UI。

**拒绝的替代方案**：

- 要求所有 provider 实现持久 journal：`notify` 后端做不到，会逼出伪实现。
- 用时间戳代替 sequence：时钟回拨与并发写入下不可靠。

**重新讨论条件**：`notify` 之外出现具备原生 journal 的 watcher 后端。

## D3：mount id 由 `VfsManager` 独占分配 {#d3-mount-id}

**结论**：`VfsManager` 持有 `next_mount_id: Arc<AtomicU64>` 并负责分配；
`VfsSnapshot::mount` 不再使用函数内 `static`；`VfsSnapshot::new` 重命名为
`with_mount_id`，仅用于采纳外部已分配身份（wire 恢复、测试）。

**理由**：

1. 当前分配器在 `VfsSnapshot::mount` 内部（`crates/vfs/src/snapshot.rs:501`），
   `VfsManager` 只是个 map，两者可以独立推进，导致 id 空间分裂。
2. `terminal_mode` 不得不手工偏移（`crates/terminal_mode/src/transfer.rs:23`、`:34`，
   `crates/terminal_mode/src/local_transfer.rs:29`），这是设计缺陷的外显。
3. `Clone` 的 `VfsManager` 共享 `Arc<AtomicU64>`，语义自然。

**拒绝的替代方案**：

- 保持静态计数器并加注释：无法在同一进程内隔离测试。
- 用 `Uuid` 直接作 `MountId`：会让热路径比较与 `BTreeMap` 键变重，且 wire 已固定
  为 `u64`。

**重新讨论条件**：需要跨进程稳定 mount 身份时（已由 D9 的 `MountKey` 覆盖）。

## D4：unmount 必须吊销已发出的身份 {#d4-revocation}

**结论**：`VfsManager::unmount` 将 `ResourceRegistry` 置为 `revoked`，清空记录并
保留 `mount_id`；所有读写入口返回 `NotFound`。返回
`UnmountOutcome { mount_id, revoked_resources }`。

**理由**：

1. 当前 `unmount` 只从 map 移除（`crates/vfs/src/manager.rs:53-61`），
   `VfsSnapshot` 是 `Clone` 的，编辑器里持有的副本仍可读写已卸载的 mount。
2. 身份失效必须是显式的，否则“资源已不存在”与“资源短暂不可用”无法区分。
3. 返回被吊销的 id 列表让 consumer 能清理自己的索引，而不是等下次访问才发现。

**拒绝的替代方案**：

- 用引用计数让 `unmount` 等待所有句柄释放：会让关闭工作区可能无限期阻塞。
- 让吊销后返回 `Cancelled`：语义上是用户取消，与卸载无关。

**重新讨论条件**：出现需要“延迟卸载直到保存完成”的场景。

## D5：资源身份是 `ResourceId`，显示路径允许 lossy {#d5-identity}

**结论**：资源身份以 `ResourceId`/`VfsPath` 为准。身份索引
`entries_by_resource_id: HashMap<ResourceId, ProjectEntryId>`（`crates/worktree/src/
worktree.rs:213`）**已经存在**，配合 `entry_for_resource_id`（`:3477-3479`）与一致性
断言（`:3741-3743`）。本阶段不是新建索引，而是把它从“附属映射”提升为唯一身份入口，
把 `entries_by_path` 降为显示与排序索引。`Entry` 用
`EntryIdentity::Exact { resource_id, vfs_path }` 或 `EntryIdentity::DisplayOnly`
承载身份，显示路径允许 lossy。

**理由**：

1. 索引存在，但写入索引之前的那一段仍以路径为准：扫描把 `ResourceRecord` 立刻转回
   `RelPath` 并对不可表示条目 `continue`（`:6179-6183`），随后又对同一个名字做
   `child_name.to_str()`（`:6224`）。索引只能容纳“已经成功变成 `RelPath`”的条目。
2. `Entry::try_from` 在路径不可表示时返回错误（`:7787-7793`），本地扫描之外的调用方
   `:3234` 用 `.log_err()` 丢掉整个条目；远端变更在 `:786-796` 也 `.log_err()`
   丢弃。这两处是当前真正的丢失点，索引本身无法补救。
3. 变更事件仍是路径基：`Event::UpdatedEntries(Vec<(Arc<RelPath>, ProjectEntryId,
PathChange)>)`（`:763`）。consumer 按路径匹配时，lossy 冲突会命中错误条目。
4. 让 `RelPath` 承载任意字节会污染整个 UI 与语言服务边界；显示与身份分离才是正确
   切分。
5. 两个不同 byte name 映射到同一显示路径时，操作仍以 `resource_id` 为准，冲突只是
   显示层的退化，不会导致误操作。

**拒绝的替代方案**：

- 保持现状（索引存在但路径仍权威）：非 UTF-8 条目在进入索引前就被丢弃，索引再全也
  看不到它们。
- 把索引改成 `BTreeMap<ResourceId, ProjectEntryId>` 以替换 `entries_by_path`：前缀
  查询、排序与渲染都依赖路径序，删掉会逼出第二套排序逻辑。
- 把 `RelPath` 改成 byte-exact：影响 `editor`、`lsp`、`search`、`git` 全部边界，
  收益低于成本。
- 直接拒绝非 UTF-8 条目并报错：比静默丢弃好，但仍不满足无损目标。
- 用显示路径加数字后缀消歧：会在 UI 上伪造文件名，误导用户与脚本。

**重新讨论条件**：语言服务或 Git 需要一个能表达任意字节的路径类型。

## D6：`Fs` 拆分去掉 supertrait，而不是新建 crate {#d6-fs-split}

**结论**：`trait Fs` 去掉 `ArchiveService + GitService + ProcessService` 三个
supertrait，只保留存储方法；`NativeExecutionContext` 暴露
`{ storage, git, jobs }` 三个字段，由同一个 `Arc<RealFs>` 分别强转。

**理由**：

1. 当前 `pub trait Fs: Send + Sync + ArchiveService + GitService + ProcessService`
   （`crates/fs/src/fs.rs:129`）让任何存储调用方都被迫接受 Git 与 job 事件。
2. 新增 crate 会在 `Cargo.toml` 与依赖图上制造长期分裂，而问题只是 trait 边界。
3. 用同一 `Arc` 多次强转意味着现有 `Arc<dyn Fs>` 调用点零改动，编译错误自动给出
   需要 git 的迁移清单。

**拒绝的替代方案**：

- 新建 `crates/storage`：会同时拖走 `FakeFs` 与测试辅助，迁移面积更大。
- 保留 supertrait 但加 blanket impl：类型系统上仍无法表达“只需要存储”。

**重新讨论条件**：`Fs` 存储方法本身也需要拆分（例如把 trash 独立出去）。

## D7：capability 决定“能否”，authorizer 决定“是否允许” {#d7-capability}

**结论**：UI 动作可用性、consumer 分支与错误预期一律由
`ProviderCapabilities` 派生；权限判定由 `VfsAuthorizer` 负责，两者不互相替代。
新增 `ResourceActionAvailability`，在渲染前一次性派生。

**理由**：

1. capability 在 `crates/vfs` 与 `crates/rpc` 之外没有读取方；UI 仍用
   `supports_trash_restore()`（`crates/worktree/src/worktree.rs:1343`）与 `is_local()`。
2. 把权限塞进 capability 会让 provider 描述里混入用户策略，破坏可缓存性。
3. 一次性派生避免每个菜单项各自判断，也便于用矩阵测试覆盖。

**拒绝的替代方案**：

- 继续用 `is_local()`：无法表达 archive、subtree、read-only 等组合。
- 用 try-and-fallback：会先显示不可用动作，再弹错误。

**重新讨论条件**：出现需要按用户而非 provider 隐藏动作的场景（届时加
`authorization` 维度，而不是复用 capability）。

## D8：`VfsUri` 使用稳定 `MountKey`，不用进程内 `MountId` {#d8-uri}

**结论**：`vfs://<mount_key>/<percent-encoded exact path>`；解析需要
`VfsManager` 提供的 `MountKey → MountId` 映射。`MountKey` 的生成见 D15。

**理由**：

1. `MountId` 是进程内计数器（D3），重启后无意义，不能进 URI。
2. percent 编码保留 `/` 分隔符，其余按字节编码，天然支持 POSIX byte name。
3. 人类可读形式由 `VfsPath::display()` 单独提供，避免 URI 承担展示职责。

**拒绝的替代方案**：

- `vfs://<mount_id>/...`：重启即失效。
- 直接用 native 绝对路径作 URI：违反“native path 只在本机暴露”。
- 用 base64 整段路径：不可读、不可 grep、难以调试。

**重新讨论条件**：需要跨用户共享的稳定资源链接（届时需要 owner 与权限模型）。

## D9：持久化用 `PersistedVfsPathV2`，不改 wire 上的 `VfsPathV2` {#d9-persistence}

**结论**：新增 proto `PersistedVfsPathV2 { mount_key, path: ExactPathV2 }` 专供
SQLite 持久化；wire 上的 `VfsPathV2 { mount_id, relative_path }`
（`crates/proto/proto/vfs.proto:52-55`）保持不变。`mount_key` 的生成与编码见 D15。

**理由**：

1. 持久化只存 `ProviderPathV2`（`crates/project/src/project.rs:465-487`），丢失 mount
   归属；重启后 archive buffer 只能靠推断恢复。
2. 直接把 `mount_id` 写进持久化会存下进程内计数器，重启后指向错误 mount。
3. 复用 `VfsPathV2` 会强迫 wire 引入 `MountKey`，扩大协议变更面。

**拒绝的替代方案**：

- 持久化 native 绝对路径：对 archive 与 remote 无意义。
- 持久化完整 descriptor：buffer 行会随 mount 配置漂移而失效。

**重新讨论条件**：wire 本身需要跨会话稳定身份时，把 `MountKey` 提升进
`VfsPathV2` 并保留兼容解码。

## D10：扩展 API 在 `since_v0.8.0` 原地扩展 {#d10-extension-api}

**结论**：在 `crates/extension_api/wit/since_v0.8.0/` 原地新增 `fs.wit` 与
`import fs;`，不切 `since_v0.9.0`。

**理由**：

1. 0.8.0 尚未进入 Stable（Stable 上限是 `since_v0_6_0`），
   `wasm_host/wit.rs:88-105` 已有 dev-only 门禁，可以承受附加变更。
2. 切新版本需要复制整套 host 实现，并为每个 export 增加 match 分支；收益只是形式上的
   版本洁净。
3. 先在一个版本内验证 API 形状，再决定是否切版本，符合“先 contract 后实现”。

**拒绝的替代方案**：

- 立刻切 `since_v0.9.0`：上千行机械改动，且 API 形状可能还要改。
- 复用 `worktree.read-text-file`：它是 `RelPath` 基的，天生 lossy，无法满足无损目标。

**重新讨论条件**：0.8.0 进入 Stable，或 API 需要不兼容变更。

## D11：`crates/vfs` 不依赖 settings {#d11-no-settings}

**结论**：预算、上限与 archive limits 通过显式参数或注入的配置结构传递；
`crates/vfs`、`crates/fs` 不新增对 `crates/settings` 的依赖。

**理由**：

1. 两个 crate 目前都不依赖 settings，保持这一点让它们可独立测试与被 `rpc` 复用。
2. settings 依赖会引入 GPUI 与全局状态，使 provider 无法在纯 async 测试中使用。
3. 现有 `ArchiveLimits::default()` 硬编码（`crates/worktree/src/worktree.rs:1292`、
   `crates/rpc/src/vfs.rs:629`）可以在调用方由 settings 构造。

**拒绝的替代方案**：

- 在 `crates/vfs` 里读全局 settings：破坏分层，且 remote server 侧没有同一份设置。
- 用环境变量：不可测试、不可审计。

**重新讨论条件**：出现必须由 provider 自身决定的上限（届时通过
`ProviderDescriptor` 携带配置快照）。

## D12：读取流化用默认方法，不新增必需方法 {#d12-streaming}

**结论**：`VfsFile` 增加 `read_stream` 默认实现（基于 `read_at`），provider 可覆盖
以获得零拷贝；新增 `ProviderLimits.maximum_whole_file_bytes` 作为整文件读取上限。

**理由**：

1. 现有 `ReadCapabilities.stream` 只是声明，没有对应方法（`crates/vfs/src/
provider.rs:79`），声明与能力脱节。
2. 默认实现让 8 个现有 provider 无需同时改动，避免一次性大面积停摆。
3. `load_provider_bytes` 现在按 `file.len()` 一次性分配（`snapshot.rs:942`），而放行
   闸门是 6 GiB（`worktree.rs:92`），是已知可致命的缺陷。

**拒绝的替代方案**：

- 只加 `maximum_whole_file_bytes` 不加流式：会让大文件彻底不可用，而不是可流式读取。
- 把 `read_stream` 设为必需方法：强迫 `ArchiveProvider` 等同步后端立刻实现复杂逻辑。

**重新讨论条件**：需要真正的背压或零拷贝跨进程传输。

## D13：remote 到 remote 直传是条件项 {#d13-remote-transfer}

**结论**：先实现 `host_id` 协商与 `TransferRouter`，只有在该 host 明确声明
`server_transfer` capability 时才走双 mount 服务端传输；否则保留客户端中转。

**理由**：

1. 当前跨 mount 一律经客户端：`transfer.rs:284-288` 只比较 `MountId` 与
   `Arc::ptr_eq`，两个远端 mount 永远走 `transfer_cross_provider`。
2. 服务端直传需要同时持有两个 mount 的授权，不能靠放宽 authorizer 实现。
3. 收益只在同 host 大文件场景，属于优化而非正确性。

**拒绝的替代方案**：

- 按 hostname 猜测同源：不安全，且 SSH/WSL/Docker 的 host 身份不可靠。
- 客户端保留字节缓存以减少重传：内存成本高，且不解决首传。

**重新讨论条件**：`host_id` 无法稳定获得，或授权模型无法在双 mount 下成立时，直接
REJECTED 并记录证据。

## D14：新 provider 的准入 {#d14-providers}

**结论**：

- `TarProvider` 必做，复用 `ArchiveProvider` 的只读与限额模板。
- `GitTreeProvider` 条件项，需 ADR 确认“无 watch、无 native path”的 UI 行为。
- `SftpProvider` / `S3Provider` 必须先有 ADR（版本、license、feature set、runtime
  影响、替代方案），否则不实施。

**理由**：

1. provider 的价值只在 conformance 套件统一后才成立，先做两个共享模板的实现。
2. 网络 provider 会引入凭据、重试、超时与离线语义，需要独立决策记录。
3. 前序 ADR 已拒绝把 OpenDAL 等库放进核心 API，本文延续该立场。

**拒绝的替代方案**：

- 一次性实现全部 provider：无法逐个验收，且依赖面失控。
- 用 feature flag 长期隐藏半成品 provider：会变成死代码。

**重新讨论条件**：出现真实用户工作流需要网络 provider，并有可测的验收标准。

## D15：`MountKey` 由 mount origin 确定性派生，不用随机数 {#d15-mount-key}

**结论**：`MountKey` 是 128 位值，由 `sha2::Sha256` 对 mount 的 **origin 描述**求
摘要后取前 128 位。内存与 wire（`PersistedVfsPathV2.mount_key`）用 16 字节原始值；
SQLite 的 `mount_key TEXT` 与 `VfsUri` 用 32 位小写十六进制表示同一个值，两者互为编码，
不要一处存字节一处存文本。origin 描述分别是：local 为
`local:<provider_id>:<native path bytes>:<path encoding>`；
remote 为 `remote:<host_id>:<project id>:<worktree id>`；archive 为
`archive:<parent mount key>:<archive resource path>:<entry name>`。origin 里不得出现
pid、时间戳或进程内 `MountId`。编码用 `sha2`（`crates/vfs` 已有依赖）加一个十行的
十六进制写出函数，不引入 `rand`、`uuid`、`base32` 等新依赖。

**理由**：

1. `crates/vfs` 当前没有随机源依赖；为了生成一个 id 就引入 CSPRNG 不划算，而弱随机
   又会诱使别人把它当凭据使用。
2. 确定性派生让重启后即使 descriptor 行丢失也能重建同一 key，D9 的恢复顺序从“必须
   先读 descriptor”降为“descriptor 可重建”。
3. 同一 origin 得到同一 key 正是需要的语义：同一资源在不同会话里是同一身份，这也是
   D8 的 URI 能跨会话复用的前提。
4. 128 位对意外碰撞足够；而 `MountKey` **不是安全凭据**，不参与授权判定，授权仍由
   `VfsAuthorizer` 负责（D7）。

**拒绝的替代方案**：

- 随机 128 位（`rand`/`getrandom`）：需要新依赖，重启不可复现，descriptor 丢失即永久
  失效。
- 直接用 `MountId` 的字符串形式：进程内计数器，重启后指向别处（D3、D8 已拒）。
- 用 worktree 名或显示路径作 key：可重命名、可冲突、可被伪造。
- 保留 base32 展示：需要一个编码依赖或手写实现，而十六进制同样可读可 grep。

**重新讨论条件**：需要跨用户共享链接或把 key 用作认证材料（届时改为签名 token，并
单独写 ADR）。

## D16：错误文案用 `VfsErrorCode` 的分类 + 静态 key，`crates/vfs` 不依赖 i18n {#d16-error-i18n}

**结论**：

- `crates/vfs/src/provider.rs` 为 `VfsErrorCode`（`:170-191`，20 个变体）提供两个静态
  方法：`message_key() -> &'static str`（如 `vfs.error.too_large`）与
  `default_message() -> &'static str`（英文兜底，与 `en.json` 一致）。
- 跨边界时把 VFS 错误映射到 `proto::ErrorCode`：新增 `FileTooLarge = 21`
  （`crates/proto/proto/zzz.proto:488`）。映射函数放在**同时依赖 `vfs` 与 `proto`**
  的 `crates/worktree`，`crates/proto` 不反向依赖 `crates/vfs`。
- UI 侧只匹配 `ErrorCode`/`VfsErrorCode` 并调用 `i18n::tr(cx, key, fallback)`；
  `project_panel` 的 `error.to_string().contains("File is too large to load")`
  （`crates/project_panel/src/project_panel.rs:1001`）改为匹配
  `ErrorCode::FileTooLarge`。
- 不新建 `crates/vfs/src/error_display.rs`，不给 `crates/vfs` 加 i18n 依赖。一致性测试
  （每个 `message_key()` 都存在于 `assets/locales/en.json`）放在同时依赖 `vfs` 与
  `i18n` 的 `crates/workspace`。

**理由**：

1. `VfsErrorCode` 已经存在且已有 wire 对应物 `VfsErrorCodeV2`
   （`crates/proto/proto/vfs.proto:98-120`，已含 `TOO_LARGE`），缺的只是展示分类。
2. 三处 `"File is too large to load"` 字面量（`crates/worktree/src/worktree.rs:1170`、
   `:2153`、`:2170`）用 `anyhow::ensure!`/`bail!` 产生无类型错误，唯一消费方只能做
   字符串包含匹配；这是完成定义 #7 明确要消除的形态。
3. `crates/vfs` 没有 i18n 依赖，加进去会把 GPUI 相关栈拖进纯 async provider 层
   （与 D11 同一理由）。返回 `&'static str` key 既保持了分层，也让 key 与枚举定义在
   同一处，新增变体时编译器会提醒补全。
4. `proto::ErrorCode` 是粗粒度 wire 枚举（`crates/proto/src/error.rs:134`
   `code_for_untyped_error` 是既有分类扩展点），只加真正需要差异化文案的变体，不把
   20 个 VFS 码全量搬过去。

**拒绝的替代方案**：

- 在 `crates/vfs` 里直接 `tr()`：需要 GPUI/i18n 依赖，provider 无法在纯 async 测试中
  使用。
- 在 `crates/proto` 里 `downcast_ref::<vfs::VfsError>()`：会让 `proto` 依赖 `vfs`，
  方向错误。
- 把 20 个 `VfsErrorCode` 全部映射成新的 `proto::ErrorCode` 变体：wire 枚举膨胀，且
  多数变体不需要不同文案。
- 继续用字符串匹配：新增错误或改文案即静默失效。

**重新讨论条件**：`crates/vfs` 被迫依赖 UI 层（那时说明 provider 抽象已经越界）。

## 未解决风险 {#open-risks}

| 风险                                  | 影响                                     | 缓解                                                  |
| ------------------------------------- | ---------------------------------------- | ----------------------------------------------------- |
| 阶段 5 身份改造面积大                 | 可能触及 editor/lsp/search 全部路径边界  | 先加索引不删旧 key；用编译错误驱动迁移                |
| 阶段 8 remote snapshot 引入延迟       | 挂载前 `None`，UI 需处理过渡态           | 过渡态按 capability 保守隐藏动作                      |
| 阶段 4 watch 语义变化                 | 短期可能出现 UI 抖动或漏更新             | 保留 `UpdateWorktree` 一个发布周期的兼容开关          |
| 阶段 11 扩展 API 改动 WIT             | 所有扩展需重新编译                       | 0.8.0 仍是 dev-only，先在 test-extension 验证         |
| 阶段 13 直传不可行                    | 性能优化落空                             | 预先定义 REJECTED 判据，不阻塞其他阶段                |
| 阶段 14 新依赖                        | 供应链与 license 风险                    | 强制 ADR，无 ADR 不合并                               |
| 账本重建依赖 `git log` 而非原始记录   | Phase 9 的真实结论可能不完整             | 只声明可验证的事实，不确定处标记 UNKNOWN 并保留旧账本 |
| remote `restore_token` 只在会话内有效 | 重启后无法 restore 之前的远端 trash 条目 | UI 按 token 缺失隐藏 restore；跨会话需新服务端 API    |
