---
title: VFS Convergence Implementation Plan
description: Execution contract for closing the ZZZ VFS integration gaps and extending the provider model.
---

# VFS 收敛与拓展实现计划

本文把 VFS 深度差距研究的结论转换成可逐条执行、可验收、可随时恢复的合同。前置
研究见[VFS 架构研究](../vfs-research/report.md)与
[VFS 重构执行计划](../vfs-research/refactoring-plan.md)；已经被合并进 `main` 的
Phase 0 至 Phase 9 不再重做，本文只处理其后的收敛与拓展。

- 计划基线：ZZZ `main` @ `40536482fa`
- 冻结决策：[decisions.md](./decisions.md)
- 执行账本：[progress.md](./progress.md)
- 可复制 Goal：[goal.md](./goal.md)
- 前序合同：[refactoring-plan.md](../vfs-research/refactoring-plan.md)
- 前序账本：[vfs-refactor-progress.md](../vfs-refactor-progress.md)

如果开始执行时 HEAD 已变化，先检查 `crates/vfs`、`crates/fs/src/vfs_provider.rs`、
`crates/rpc/src/vfs.rs`、`crates/worktree/src/worktree.rs` 的新提交，记录新
baseline 后继续。不得 reset、clean、checkout 覆盖或丢弃现有修改。

## 1. 目标 {#goal}

一句话目标：把已经正确但**只完成一半接线**的 VFS 变成产品实际使用的唯一文件抽象，
并在此之上补齐身份、生命周期、可观测性与可拓展面。

前序重构证明了 provider、snapshot、无损 path wire 和 cross-provider transfer 是可
工作的。剩余缺口不是缺少模块，而是三类：

1. **投影回退**：worktree 取得 VFS 记录后立刻转回 `RelPath` 与原生绝对路径，身份、
   非 UTF-8 名称和 mount 归属在第一次跨边界时被丢弃。
2. **没有驱动者**：`VfsSnapshot::apply_event_batch` 没有产品调用点，worktree 仍以
   `fs.watch` 与 `UpdateWorktree` 为主路径。
3. **契约无人消费**：`ProviderCapabilities` 在 `crates/vfs` 与 `crates/rpc` 之外没有
   读取方，UI 仍以 `supports_trash_restore()` 和 `is_local()` 决定动作。

## 2. 完成定义 {#done}

只有同时满足以下条件，整个 Goal 才能标记完成：

1. Phase 0 至 Phase 16 的必做项全部完成；所有条件项有“实施”或“拒绝并附实验证据”
   的明确结论。
2. 核心层没有已知退化路径：snapshot 预算摊销为 O(1)、目录分页成本与目录规模解耦、
   整文件读取有显式上限、`read_range` 有 single-flight、watch sequence 缺口可检测。
3. mount 生命周期由 `VfsManager` 独占：`crates/vfs` 之外不出现 `MountId::new`；
   `unmount` 吊销已经发出去的 `VfsSnapshot` 与 `ResourceId`。
4. watch 由 supervisor 驱动：`apply_event_batch` 至少有一个产品调用点。`fs.watch` 仍可
   作为本地事件源，但 worktree 的正确性不再依赖它的事件完整性。
5. worktree 的资源身份是 `ResourceId`/`VfsPath`；非 UTF-8 名称在扫描、面板、编辑器
   与文件操作中端到端可见可操作；扫描路径不存在 `to_str()` 往返。
6. capability 是 UI 动作可用性的唯一来源；`supports_trash_restore()` 与
   `is_local()` 不再决定文件动作；remote trash 与 restore 可用。
7. 需要差异化文案的 VFS 错误类别（至少 `TooLarge`、`PermissionDenied`、
   `ReadOnly`、`Disconnected`）有 typed 分类与 i18n 文案；不存在按错误文本匹配的
   分支。
8. `Fs` 只承担存储职责；Git、archive extraction 与 job reporting 是独立 trait；
   `NativeExecutionContext` 暴露三个独立字段。
9. remote worktree 在客户端拥有 `VfsSnapshot`；`vfs_snapshot()` 不再以
   `as_local()?` 为前提。`vfs_authorizer()` 保持“把本地 worktree 提供给远端对端”的
   服务端语义，不为 remote 客户端伪造授权。
10. mount descriptor 持久化后，archive 与 remote buffer 重启可恢复；持久化格式包含
    稳定 mount key，而不是进程内计数器。
11. `VfsUri` 编解码往返无损，覆盖 POSIX byte name 与 Windows encoding，并有跨平台
    测试。
12. 扩展只能通过授权沙箱读取 VFS；越界返回 `PermissionDenied`，不泄漏 native path。
13. 写入语义诚实：原子保存由 capability 决定；`set_times`、`create_hard_link`、
    `stat_many` 由 capability 声明并由 consumer 消费，不是只声明。
14. remote 到 remote 直传已实施，或带有 `CONV-EXP-017` 证据的 REJECTED 结论。
15. 至少落地一个新 provider（Tar），其余新 provider 有实验与拒绝结论。
16. code analysis 与搜索消费 `SnapshotDelta`，不再依赖全量重扫维持新鲜度。
17. 没有新增 `unwrap()`、`let _ =`、`mod.rs`、无 owner 后台任务。`./script/clippy`、
    `cargo test --workspace`、`./script/check-philosophy`、`./script/check-todos`、
    `cargo fmt --all -- --check`、docs Prettier 与 mdBook build 通过，或只剩记录在案
    并从 clean baseline 复现的既有失败。
18. 执行账本记录每阶段的 baseline、决策、改动、实验、验证命令与 exit status、提交
    SHA 和下一步；旧账本已归档并指向新账本。

## 3. 范围 {#scope}

必改核心：

- `crates/vfs`
- `crates/fs`
- `crates/proto`
- `crates/rpc`
- `crates/worktree`
- `crates/project`
- `crates/remote_server`
- `crates/remote`

按阶段触及：

- `crates/project_panel`
- `crates/editor`
- `crates/workspace`
- `crates/terminal_mode`
- `crates/extension_api`
- `crates/extension_host`
- `crates/extension`
- `crates/settings_content`、`crates/settings`
- `crates/audio_viewer`、`crates/pdf_viewer`、`crates/video_viewer`
- `crates/search`、`crates/git`
- `assets/locales/en.json`、`assets/locales/zh-CN.json`
- `docs/src/development/vfs-convergence`

明确不在范围内：

- 更换 GPUI、Smol 或全局 async runtime；
- FUSE / WinFSP / OS 级 mount；
- 恢复账号、遥测、托管协作或默认网络；
- 可写 ZIP；
- 重写 Project Panel 视觉设计；
- 把 `std::PathBuf`、URI string 或 `to_string_lossy` 变成核心身份。

## 4. 执行规则 {#execution-rules}

### 4.1 工作区与提交 {#workspace}

1. 开始前记录 `git status`、HEAD、分支与现有 untracked/modified files。
2. 在隔离 worktree `../ZZZ-vfs-convergence` 上以分支 `vfs-convergence` 工作；
   建立方式见[账本](./progress.md#bootstrap)。
3. 保留用户修改。不得执行 destructive reset、clean 或覆盖命令。
4. 不 push、不开 PR。合并回 `main` 只在全部完成定义满足且用户明确确认后执行。
5. 每个提交只包含一个可独立审查与回退的结构或行为变化，使用 `git commit -s`。
   阶段结束前不留下不编译的提交。
6. 原始 benchmark、fixture、失败注入输出放在 `.tmp/vfs-convergence/`；只把可复现
   命令、摘要与结论写进 tracked docs。
7. 每个阶段结束用 `hunting-code-smells` 审查 touched diff，修复确认的异味后验收。
8. 若移植 Zed upstream commit，先走 `absorbing-upstream` 的 A/B/C 分类；不得把原创
   重构伪装成 upstream absorption。

### 4.2 代码规范 {#code-standards}

- 遵守根 `AGENTS.md`、`.rules`、`clippy.toml` 与触及 crate 的局部规则。
- 不新增 `unwrap()`；不用可能 panic 的索引。
- 不用 `let _ =` 丢弃 fallible operation。
- 不创建 `mod.rs`；新逻辑组件用描述性文件名。
- 使用完整变量名，不使用无意义缩写。
- `Entity::update` / `update_in` closure 使用内部 `cx`，不重入同一 entity。
- 所有 `cx.spawn` / `cx.background_spawn` Task 必须 await、detach 或存储。
- GPUI tests 使用 executor timer，不使用 `smol::Timer::after`。
- 用户可见文本走 `i18n::tr`，同步 `en.json` 与 `zh-CN.json`，两端 key 集合一致，
  in-code fallback 与 `en.json` 完全一致。
- 公共类型先写 observable contract 再写实现；provider 细节不泄漏给 consumer。
- 错误保留 operation、mount、source chain，但不记录 credential 或文件内容。
- 默认构建不启用 benchmark、trace、journal 或 fault-injection 开销。

### 4.3 架构不变量 {#invariants}

以下不变量强于任何阶段的局部便利：

1. path identity 与 display string 分离；display 可以是 lossy，identity 不可以。
2. consumer 不根据 local/remote/archive 选择文件操作实现。
3. capability 描述“能否做”，authorizer 描述“是否允许做”；两者不可互相替代。
4. watcher event 是提示，snapshot reconciliation 是真相。
5. `ResourceId` 只在 mount 存活期间有效；unmount 后必须报 `NotFound`。
6. remote operation 可取消；断线不遗留不可回收 handle 或未完成 write。
7. write、rename、delete 不虚构 atomicity；无法保证时返回明确语义。
8. cross-provider move 只有在 copy 验证成功后才允许删除源。
9. native path 只在 resource 所在 execution host 暴露，永不作持久化身份。
10. archive 与虚拟 provider 只读，默认不 materialize、不执行 entry、不跟随 symlink。
11. cache 有容量、版本与 eviction；stale cache 不作为成功结果静默返回。
12. 每个 compatibility adapter 必须有调用点清单、删除阶段和测试。
13. 同一事实只有一个 owner；两个 reconciliation 引擎并存视为缺陷而非特性。

### 4.4 依赖政策 {#dependencies}

- 不引入新的 path 或 URI 基础库；`VfsUri` 用 `crates/vfs` 内部 codec 实现。
- `cap-std` / `openat2` 只在 provider conformance 与平台测试通过后用于 local
  containment；不进入核心身份。
- S3/SFTP provider 若需要新依赖，必须先写 ADR 记录版本、license、feature set、
  runtime 影响与替代方案，再实施；无 ADR 不得合并。
- `crates/vfs` 与 `crates/fs` 不新增对 `crates/settings` 的依赖；上限与预算通过显式
  参数或注入的配置结构传递。

## 5. 阶段总览 {#phase-overview}

| 阶段 | 结果                       | 进入条件   | 退出条件                   |
| ---- | -------------------------- | ---------- | -------------------------- |
| 0    | 账本校准、worktree、基线   | 工作树安全 | CONV-EXP-001 PASS          |
| 1    | 可观测性与度量门禁         | 阶段 0     | CONV-EXP-002 可测量        |
| 2    | 核心退化修复               | 阶段 1     | CONV-EXP-003/004/005 PASS  |
| 3    | mount 生命周期             | 阶段 2     | CONV-EXP-008 PASS          |
| 4    | WatchSupervisor 与 delta   | 阶段 3     | CONV-EXP-006 PASS          |
| 5    | worktree 精确身份          | 阶段 4     | CONV-EXP-007/009 PASS      |
| 6    | capability 驱动与错误 i18n | 阶段 5     | CONV-EXP-010/011 PASS      |
| 7    | `Fs` 职责拆分              | 阶段 6     | CONV-EXP-012 PASS          |
| 8    | remote 客户端 snapshot     | 阶段 7     | CONV-EXP-013 PASS          |
| 9    | mount descriptor 持久化    | 阶段 8     | CONV-EXP-014 PASS          |
| 10   | `VfsUri` 与 codec          | 阶段 9     | CONV-EXP-015 PASS          |
| 11   | 扩展只读 FS API            | 阶段 10    | CONV-EXP-016 PASS          |
| 12   | 写入语义补全               | 阶段 6     | CONV-EXP-018 PASS          |
| 13   | remote 到 remote 直传      | 阶段 8     | CONV-EXP-017 PASS/REJECTED |
| 14   | 新 provider（Tar 必做）    | 阶段 6     | CONV-EXP-019 PASS          |
| 15   | 增量索引与 delta 消费      | 阶段 5     | CONV-EXP-020 PASS          |
| 16   | 收敛、全量验证、合并       | 前述完成   | 完成定义全部满足           |

依赖只写真正的前置：阶段 12 需要阶段 6 的 capability 消费，阶段 13 需要阶段 8 的
remote snapshot，阶段 14 需要阶段 6（只读 provider 的 UI 行为由 capability 决定），
阶段 15 需要阶段 5 的 `ResourceId` 身份。因此 12 与 13 可与 10、11 并行；串行执行时按
编号顺序。

## 6. 阶段 0：账本校准与基线 {#phase-0}

### 目标 {#phase-0-goal}

让后续所有阶段有可信的起点。当前 `vfs-refactor-progress.md` 声称分支
`vfs-refactor`、Phase 9 IN PROGRESS、边界 `fbf39ad807`，而 `main` 已在其后 366 个
提交。账本与现实不一致本身就是完成定义 #18 的不满足项。

### 工作 {#phase-0-work}

1. 先把本目录四份文档作为 `main` 上的一个独立提交落地（`docs: add VFS convergence
plan`，`git commit -s`）。`git worktree add ... main` 只会检出已提交内容，文档未提交
   时新 worktree 里没有计划，Goal 会立刻卡住。
2. 用 `git log --oneline fbf39ad807..HEAD -- crates/vfs crates/fs/src/vfs_provider.rs
crates/rpc/src/vfs.rs crates/worktree/src/worktree.rs crates/proto/proto/vfs.proto`
   重建 Phase 9 的真实提交清单。
3. 在旧账本顶部加归档段：`Status: ARCHIVED`、终止提交、Phase 9 的实际结论、指向
   新账本。保留原文，不删除历史。
4. 建 worktree 与分支（`main` 现在已包含计划文档）：

```sh
git worktree add ../ZZZ-vfs-convergence -b vfs-convergence main
```

5. 记录 baseline：HEAD、`rustc -Vv`、OS、filesystem 类型、case sensitivity、
   remote transport、`git status --porcelain` 为空。代码基线仍是 `40536482fa`，
   文档提交只改变 HEAD。
6. 运行并记录 exit status：

```sh
cargo check --locked -p vfs -p fs -p worktree -p project -p rpc
cargo test --locked -p vfs -p fs -p worktree -p project -p rpc
./script/clippy -p vfs -p fs -p worktree -p project -p rpc
```

7. 运行 `CONV-EXP-001`，把垂直链 baseline 写入账本：初始扫描、展开、打开文本、打开
   二进制、保存、rename、trash、remote 扫描、reconnect。

### 退出条件 {#phase-0-exit}

账本分支、HEAD、阶段状态与现实一致；基线命令有 exit status；worktree 可用；
`CONV-EXP-001` PASS。

## 7. 阶段 1：可观测性与度量门禁 {#phase-1}

### 目标 {#phase-1-goal}

在改性能与语义之前先能测量。当前 `crates/vfs/src` 中 `tracing` 出现次数为 0，任何
“变快了”或“没退化”的结论都不可复现。

### 工作 {#phase-1-work}

1. `crates/vfs` 的每个公开操作加 `tracing::debug_span!`，字段固定为
   `mount_id`、`provider_id`、`operation`、`path_display`、`path_encoding`。不记录
   credential，不记录文件内容，不对每个目录项打点。
2. `SnapshotStats`（`crates/vfs/src/snapshot.rs:88-100`）已经包含
   `identity_count`、`metadata_count`、`estimated_*_bytes`、`cached_range_count`、
   `last_sequence` 与 `rescan_state`；只补齐缺失计数：`interns`、`evictions`、
   `range_cache_hits`、`range_cache_misses`、`in_flight_reads`。计数只增，可从
   `VfsSnapshot::stats()` 读出。
3. `VfsManager` 暴露 `mount_count()` 之外的 `mounts()` 迭代器骨架（阶段 3 补全语义）。
4. 度量脚本：在 `docs/src/development/vfs-convergence/progress.md` 记录三条可复制的
   `rg` 门禁命令与其当前值，作为后续阶段的单调性检查。
5. 确认默认构建无额外开销：span 使用 `debug_span!`，计数使用 `Relaxed` 原子。

### 交付物 {#phase-1-deliverables}

- `crates/vfs/src/snapshot.rs` 的计数与 span
- `crates/vfs/src/manager.rs` 的 `mounts()` 骨架
- 账本中的度量基线表

### 退出条件 {#phase-1-exit}

`CONV-EXP-002` 能在同一台机器上复现两次并给出相同量级结果；`./script/clippy -p vfs`
通过；没有为埋点引入新依赖。

## 8. 阶段 2：核心退化修复 {#phase-2}

本阶段只改实现，不改公开语义。每项都必须有独立实验，且实验先于修复提交。

### 2.1 snapshot 预算 O(1) 摊销 {#phase-2-budgets}

现状：`intern_metadata` 每次调用 `enforce_budgets`，后者两次遍历全部
`state.records` 求 `sum`（`crates/vfs/src/snapshot.rs:296`、`1289-1318`）；
`reconcile_directory` 对 N 个条目调用 N 次。单目录 100k 文件即 O(N²)。

工作：

1. 在 `ResourceRegistryState` 增加 `directory_bytes` 与 `metadata_bytes` 增量计数。
2. 把所有对 `records` 的变更收敛到一个 `apply_record_change(state, id, old, new)`
   入口，计数只在该入口更新，避免散落的加减。
3. `enforce_budgets` 只在计数器超限时触发；淘汰按 `last_access` 顺序进行，不每次
   `sort_unstable` 全表。
4. `debug_assert_eq!` 交叉校验增量计数与重新求和的结果（仅测试构建）。
5. 单测：`intern` 10k 次后计数与淘汰数一致；更新 metadata 后计数不漂移。

### 2.2 目录分页成本与目录规模解耦 {#phase-2-paging}

现状：`LocalProvider::read_directory` 每页从 `read_dir` 头部全量迭代，cursor 只用于
`continue` 过滤，每个保留条目再 `stat` 一次
（`crates/fs/src/vfs_provider.rs:502-534`）。

工作：

1. `LocalProvider` 增加按目录缓存的页快照：排序后的 `Arc<Vec<ExactComponent>>` 加
   `structure_version` 与捕获时间。
2. 每页先 `stat` 目录一次（O(1)）；`structure_version` 未变则从缓存切片，变了则
   重建。缓存有容量上限，按 LRU 淘汰。
3. cursor 语义不变（最后一个 component 的字节），保证跨 provider 实例可用；缓存
   未命中时回退到全量扫描，正确性优先。
4. 目标：`directories.paged` 从 `Emulated` 升为 `Native`，且 `CONV-EXP-004` 证明第
   25 页成本不随目录规模线性增长。

### 2.3 整文件读取有显式上限 {#phase-2-whole-file}

现状：读取循环本身已经按 `maximum_range_size` 分块
（`crates/vfs/src/snapshot.rs:944-950`），但缓冲区在第一次读取之前就按 `file.len()`
一次性分配（`:941` 的 `vec![0; length]`），而 worktree 的放行闸门是 6 GiB
（`crates/worktree/src/worktree.rs:92`）。因此 6 GiB 文件会先要求 6 GiB 连续内存，
OOM 发生在任何错误处理之前；缺少的不是分块读取，而是**整文件上限**与**增量缓冲**。

工作：

1. `VfsFile` 增加 `read_stream` 默认方法：基于 `read_at` 的
   `Stream<Item = VfsResult<Bytes>>`，块大小为 `maximum_range_size`。provider 可
   覆盖以获得零拷贝（`MemoryProvider` 在 `contents` 改为 `Arc<[u8]>` 后可直接切片）。
   `crates/vfs` 需要新增 `bytes.workspace = true`（`bytes` 已在 workspace 依赖中，
   `Cargo.toml:515`，不是新的第三方依赖）。
2. `ProviderLimits` 增加 `maximum_whole_file_bytes: NonZeroU64`；`LocalProvider` 设
   为 512 MiB，`MemoryProvider` 与 `ArchiveProvider` 按各自上限设置。该字段需要
   同步到 wire：`VfsProviderLimitsV2` 增加 `maximum_whole_file_bytes = 6`
   （`crates/proto/proto/vfs.proto:240-246`），并在 `crates/rpc/src/vfs.rs` 的
   capability 转换两侧补齐，缺字段时按默认值解码。
3. `load_provider_bytes` 改为增量增长（按 chunk `extend_from_slice`，或先取
   `file.len()` 仅做上限判断再按实际读到的长度分配），并且一旦总长超过
   `maximum_whole_file_bytes` 立即返回 `TooLarge`。不得在读取前按总长预分配。
4. `ReadCapabilities.stream` 从“声明”变为“可调用”：`Native` 表示 provider 覆盖了
   `read_stream`，`Emulated` 表示默认实现。
5. 把 `MAX_FILE_LOAD_BYTES` 从 6 GiB 改为由 `ProviderLimits.maximum_whole_file_bytes`
   驱动（worktree 不再持有独立常量），失败时走 typed error，由阶段 6 接到 i18n 文案。

### 2.4 `read_range` single-flight {#phase-2-single-flight}

现状：每次 `read_range` 先 `stat`，再 `intern_metadata`（触发 2.1 的全量扫描），再查
缓存（`crates/vfs/src/snapshot.rs:581-604`）。

工作：

1. 在 `range_cache` 旁挂 `in_flight: Mutex<BTreeMap<RangeCacheKey, Shared<Future>>>`。
2. 命中 in-flight 直接 await，不分叉重复 I/O。
3. 缓存键已经包含 `source_version`，不需要额外失效逻辑。
4. 单测：同一 range 并发 8 次只产生一次 `open`（用计数 provider 断言）。

### 2.5 大小写与 lookup 解析 {#phase-2-case}

现状：`LookupKey` 只在 `reconcile_directory` 内部用于重复检测
（`crates/vfs/src/snapshot.rs:1008-1030`），不参与解析；`case_sensitivity` 已声明但
无人使用。

工作：

1. `ResourceRegistry` 增加 `lookup(path, options) -> Option<ResourceId>`，在
   `Insensitive` 时按 `LookupKey` 归一化。
2. 大小写冲突复用已有的 `VfsErrorCode::NameCollision`（`crates/vfs/src/provider.rs:181`，
   wire 侧已有 `VFS_ERROR_CODE_V2_NAME_COLLISION = 20`），**不新增变体**；detail 里标明是
   case-insensitive 冲突，并带上两个冲突名。
3. `EntryMetadata.case_sensitivity` 与 `ProviderCapabilities.case_sensitivity` 参与
   决策，不硬编码平台判断。
4. 单测覆盖：`Insensitive` 下 `foo.rs` 与 `Foo.rs` 命中同一 `ResourceId`；`Sensitive`
   下是两个不同 id。

### 退出条件 {#phase-2-exit}

`CONV-EXP-003`（预算）、`CONV-EXP-004`（分页）、`CONV-EXP-005`（整文件内存）PASS；
`cargo test -p vfs -p fs` 全绿；公开 API 只增不改语义。大小写的端到端验收（`foo.rs`
与 `Foo.rs` 在真实文件系统上产生 `NameCollision`）属于阶段 5 的 `CONV-EXP-009`，本阶段
只要求 `LookupKey` 归一化的单测通过；平台相关部分写 runbook 并标记 NOT RUN。

## 9. 阶段 3：mount 生命周期 {#phase-3}

### 目标 {#phase-3-goal}

让 mount id 有唯一分配者，`unmount` 真正吊销身份。

### 现状 {#phase-3-status}

- `VfsSnapshot::mount` 用函数内 `static NEXT_MOUNT_ID`
  （`crates/vfs/src/snapshot.rs:500-514`）。
- `VfsSnapshot::new` 接受任意 id（`:518`）。
- `terminal_mode` 手工偏移避免冲突（`crates/terminal_mode/src/transfer.rs:23`、`:34`，
  `crates/terminal_mode/src/local_transfer.rs:29`）。
- `VfsManager::unmount` 只从 map 移除，不吊销已发出的 snapshot
  （`crates/vfs/src/manager.rs:53-61`）。

### 工作 {#phase-3-work}

1. `VfsManager` 增加 `next_mount_id: Arc<AtomicU64>`；`mount()` 由 manager 分配 id。
2. `VfsSnapshot::mount` 改为不需要外部 id 的内部构造；`VfsSnapshot::new` 重命名为
   `with_mount_id`，文档说明只用于“采纳外部已分配身份”（wire 恢复、测试）。
3. `ResourceRegistry` 增加 `revoke()`：置 `revoked` 标志，清空 `records`，保留
   `mount_id`。所有读写入口在已吊销时返回 `NotFound`，detail 为
   `"VFS mount has been unmounted"`。
4. `VfsManager::unmount` 返回 `UnmountOutcome { mount_id, revoked_resources }`；
   `mounts()` 返回 `(MountId, ProviderDescriptor)`。
5. `terminal_mode` 改为向 manager 申请 mount id，删除两处 magic offset。
6. 单测：unmount 后旧 `VfsSnapshot` 句柄的所有操作返回 `NotFound`；重复 unmount 返回
   `NotFound`；`mounts()` 不包含已卸载项。

### 退出条件 {#phase-3-exit}

`rg 'MountId::new' crates --glob '*.rs'` 在 `crates/vfs/src/manager.rs` 之外为 0（测试
模块除外，且测试须注明理由）；`CONV-EXP-008` PASS。

## 10. 阶段 4：WatchSupervisor {#phase-4}

### 目标 {#phase-4-goal}

把 provider watch 与 snapshot reconciliation 接起来，让 worktree 消费 delta 而不是
自己维护第二套 diff。

### 现状 {#phase-4-status}

- provider 侧完整：`crates/fs/src/vfs_provider.rs:720 watch_path`。
- RPC 侧完整：`crates/remote_server/src/headless_project.rs:615-630`、
  `crates/project/src/project.rs:4977`。
- 产品侧零订阅：`apply_event_batch` 只在 `crates/rpc/src/vfs.rs:4217/4254/4294`
  的测试里调用。
- sequence 语义偏弱：一个批次共用一个 sequence（`first == last`），`resume` 一律先发
  `Overflow`（`crates/fs/src/vfs_provider.rs:733-745`、`:771-775`）。

### 工作 {#phase-4-work}

1. 新逻辑组件 `crates/vfs/src/watch_supervisor.rs`：

```rust
pub struct WatchSubscription {
    pub deltas: BoxStream<'static, VfsResult<SnapshotDelta>>,
}

impl VfsSnapshot {
    pub fn watch(&self, request: WatchRequest) -> VfsResult<WatchSubscription>;
}
```

2. supervisor 负责：订阅 provider、检测 sequence 缺口（`first_sequence >
last_seen + 1` 视为丢事件）、`Overflow` 时对 `rescan_root` 执行
   `load_directory` 全量重建、`Disconnected` 时按退避重连。
3. `resume_after_sequence` 只在 `WatchCapabilities.resumable_journal == Native` 时传递；
   否则重连后直接按 `Overflow` 处理，避免假装可恢复。
4. provider 侧 `next_watch_sequence` 改为每个事件一个 sequence，`EventBatch` 的
   `first/last` 表达真实区间；`resume_after_sequence` 在本地 provider 上仍返回
   `Overflow`，但 detail 说明原因。
5. 单测（`MemoryProvider` 有 journal）：缺口检测、overflow 重建、订阅 drop 后 watcher
   释放、重连后终态一致。

### 退出条件 {#phase-4-exit}

`CONV-EXP-006`（watch 缺口与恢复）PASS；`apply_event_batch` 有 supervisor 调用点；
`crates/vfs` 无 `unwrap`、无无界 channel、无未 detach 的 spawn。

## 11. 阶段 5：worktree 精确身份 {#phase-5}

这是本计划风险最高、收益最大的一阶段。

### 现状 {#phase-5-status}

- 身份索引**已经存在**：`entries_by_resource_id: HashMap<ResourceId,
ProjectEntryId>`（`crates/worktree/src/worktree.rs:213`），配合
  `entry_for_resource_id`（`:3477-3479`）与一致性断言（`:3741-3743`）。所以本阶段不是
  新建索引，而是消除仍然以路径为主的说法与丢弃点。
- 扫描已经走 `load_directory`（`:6173`），但随即 `provider_path_to_legacy_utf8` 逐条
  转回 `RelPath`，不可表示的条目被 `continue` 静默丢弃（`:6179-6183`）。
- 之后又对同一个名字做一次 `child_name.to_str()`（`:6224`），身份由
  `Entry.path: Arc<RelPath>` 承载。
- 远端条目已经带精确身份（`proto::Entry` 的 `vfs_path = 15`、`resource_id = 16`，
  `crates/proto/proto/worktree.proto:40-41`），但 `Entry::try_from` 在路径不可表示时
  直接返回错误（`:7787-7793`），调用方 `:3234` 用 `.log_err()` 把整个条目丢掉。
- 变更事件仍是路径基：`Event::UpdatedEntries(Vec<(Arc<RelPath>, ProjectEntryId,
PathChange)>)`（`:763`），远端更新在 `:786-796` 把不可表示的路径 `.log_err()` 丢弃。
- `Entry` 已经预留 `resource_id: Option<ResourceId>` 与 `vfs_path: Option<VfsPath>`
  （`:4693-4700`），但仍可与 `path` 不一致。
- `:6373` 存在 `expect("path should be valid UTF-8")`，今天是死代码，一旦放宽上游
  过滤器就是 panic。

### 工作 {#phase-5-work}

1. 把已有的 `entries_by_resource_id` 提升为唯一身份入口：文件操作、选择、撤销、拖放
   一律以 `ProjectEntryId` → `Entry.resource_id` → `VfsPath` 解析，不经
   `entries_by_path` 反查。`entries_by_path` 只用于显示、排序与前缀查询。
2. 扫描不再丢弃不可表示的条目：`Entry.vfs_path` 来自 `ResourceRecord.path`（精确），
   `Entry.path` 使用 lossy 显示形式。当两个不同 byte name 映射到同一显示路径时，保留
   第一个并 `tracing::warn!` 记录冲突计数；操作仍以 `resource_id` 为准。
3. 删除 `child_name.to_str()` 往返：子条目的 `ResourceId` 与精确路径直接来自
   `ResourceRecord`，不再从原生绝对路径重新推导。
4. 删除 `:6373` 的 `expect`，改为使用已解析的 `ResourceId`。
5. 用 `EntryIdentity` 取代 `Entry` 上并列的两个 `Option`
   （`crates/worktree/src/worktree.rs:4696-4697`），消除“有 id 没路径”和“有路径没 id”
   这两种非法组合：

   ```rust
   pub struct Entry {
       // ...
       identity: Option<EntryIdentity>,
   }

   pub enum EntryIdentity {
       Exact { resource_id: ResourceId, vfs_path: VfsPath },
       DisplayOnly { resource_id: ResourceId },
   }
   ```

   外层 `Option` 表示“这个 worktree 没有 VFS snapshot”（遗留路径），内层枚举表示
   “有 snapshot，但精确路径不可表示给 UI”。不要在 `Entry` 上再加第三个字段。

6. `Entry::try_from` 对不可表示路径不再返回错误：构造 display-only 身份并保留精确
   `vfs_path`，让远端条目也能出现在树里。
7. 让变更事件能表达精确身份：`Event::UpdatedEntries` 的载荷加入 `ResourceId`（或改为
   携带 `EntryIdentity` 的枚举），`project_panel`、`workspace` 与 rename 跟随的消费方
   按 id 而不是按路径匹配。远端这批改动与阶段 8 一起验收。
8. `relative_path_from_vfs_path` 保留，但明确返回 `Unrepresentable` 语义，由 UI 决定
   退化显示，不再在扫描层静默丢。

### 退出条件 {#phase-5-exit}

`CONV-EXP-007`（非 UTF-8 端到端）与 `CONV-EXP-009`（case collision）PASS；扫描路径中
`to_str()`/`provider_path_to_legacy_utf8` 调用数为 0（用 `rg` 门禁断言）；本地扫描与
远端更新都不再出现 `.log_err()` 丢弃条目；worktree 全量测试通过。

## 12. 阶段 6：capability 驱动与错误 i18n {#phase-6}

### 工作 {#phase-6-work}

1. `Worktree::capabilities() -> Option<ProviderCapabilities>`：local 从 snapshot 的
   descriptor 取；remote 在 `try_remote` 时发一次 `VfsNegotiateRequestV2` 并把
   `VfsNegotiateResponseV2.capabilities` 缓存进 `RemoteWorktree`（RPC 两侧都已存在：
   `crates/remote_server/src/headless_project.rs:448`、`crates/rpc/src/vfs.rs:464`）。
   取不到返回 `None`，UI 保守隐藏动作。
2. `File.is_local = vfs_snapshot.is_some()`（`crates/worktree/src/worktree.rs:4619`）改为
   `capabilities().links.native_path.is_supported()`。该 capability **已经存在**
   （`crates/vfs/src/provider.rs:136`；wire 侧 `VfsLinkCapabilitiesV2.native_path = 5`，
   `crates/proto/proto/vfs.proto:232`），`LocalProvider` 报 `Native`
   （`crates/fs/src/vfs_provider.rs:1726`），trait 默认实现返回 `Unsupported`，因此
   `RemoteProviderProxy` 与 `ArchiveProvider` 天然是 `Unsupported`。不要新增
   `native_paths` 之类的重复字段。“有 snapshot” 不等于“有原生路径”：阶段 8 之后 remote
   worktree 也有 snapshot，但它的 `abs_path` 在客户端无意义。
3. `supports_trash_restore()` 改为 `capabilities().trash.restore.is_supported()`；
   删除 `matches!(self, Local(_))`（`crates/worktree/src/worktree.rs:1343`）。
4. remote trash 必须带回 restore token。服务端 `handle_delete_project_entry`
   （`crates/project/src/worktree_store.rs:1323`）本地已经产生 `TrashedEntry`，但响应
   `proto::ProjectEntryResponse`（`crates/proto/proto/worktree.proto:177-180`）只有
   `entry` 与 `worktree_scan_id`，客户端因此只能返回 `None`
   （`crates/worktree/src/worktree.rs:2897-2904` 的 TODO）。给响应增加可选
   `TrashedEntryV2 trashed = 3 { ProviderPathV2 original_path = 1; bytes restore_token = 2; }`，
   由服务端填充、客户端 remote 分支保存。不改 `DeleteProjectEntry` 请求，不新增第二个
   RPC，也不重复执行 trash。
5. remote restore：`Worktree::restore_entry`（`:1497-1507`）的 remote 分支发
   `VfsRestoreRequestV2`（`crates/proto/proto/vfs.proto:582-588`），token 来自上一条，
   `original_path` 用 `vfs_root.mount_id()`；`RemoteProviderProxy::restore`
   （`crates/rpc/src/vfs.rs:2824`）已是这条请求的封装。删除 `TODO(dino)`，不需要等阶段
   8 的客户端 snapshot。
6. `project_panel` 引入一次计算的 `ResourceActionAvailability`：
   `{ trash, restore, rename, reveal_native, open_in_terminal, duplicate, download }`，
   全部由 capability 与 `Project::is_read_only` 派生。删除
   `is_local`/`is_remote`/`supports_trash_restore` 的动作级判断。`restore` 额外要求当前
   会话仍持有 `restore_token`（见下一条限制）。
7. 错误文案按 D16：`crates/vfs/src/provider.rs` 的 `VfsErrorCode` 增加
   `message_key()`/`default_message()`；`proto::ErrorCode` 增加 `FileTooLarge = 21`；
   映射函数放在同时依赖两边的 `crates/worktree`；三处
   `"File is too large to load"` 字面量（`crates/worktree/src/worktree.rs:1170`、
   `:2153`、`:2170`）改为构造 typed error，`project_panel` 的
   `error.to_string().contains(...)`（`crates/project_panel/src/project_panel.rs:1001`）
   改为匹配 `ErrorCode::FileTooLarge`。不建 `crates/vfs/src/error_display.rs`，不给
   `crates/vfs` 加 i18n 依赖。
8. 同步 `en.json` 与 `zh-CN.json`，只为需要差异化文案的类别加 key（至少 `TooLarge`、
   `PermissionDenied`、`ReadOnly`、`Disconnected`、`NotFound`、`Conflict`），不要求
   20 个全量；fallback 与 `en.json` 完全一致。key 集合一致性测试放在同时依赖 `vfs` 与
   `i18n` 的 `crates/workspace`。

### 已知限制 {#phase-6-limits}

`restore_token` 是不透明字节，服务端没有“列出回收站”的 API，因此它只在当前会话内有效。
客户端重启后无法 restore 本会话之前 trash 的 remote 条目；`ResourceActionAvailability`
的 `restore` 在 token 缺失时为 `false`。跨会话 restore 需要新的服务端 API 与决策，不在
本计划范围内；该限制记入[账本](./progress.md#known-limits)与
[decisions.md](./decisions.md#open-risks)。

### 退出条件 {#phase-6-exit}

`CONV-EXP-010`（capability × action 矩阵）与 `CONV-EXP-011`（错误码分类与文案）PASS；
`rg 'supports_trash_restore|contains\("File is too large'` 在产品代码中为 0；remote
trash 后 restore 有集成测试（同一会话内往返）；`links.native_path` 为 `Unsupported` 的
mount 上 `reveal_native`/`open_in_terminal` 不可用。

## 13. 阶段 7：`Fs` 职责拆分 {#phase-7}

### 现状 {#phase-7-status}

`pub trait Fs: Send + Sync + ArchiveService + GitService + ProcessService`
（`crates/fs/src/fs.rs:129`），`RealFs` 与 `FakeFs` 各自实现全部四份
（`:1380`、`:1472`、`:3533`、`:3575`）。

### 工作 {#phase-7-work}

1. 先改类型不改行为：`trait Fs` 去掉 supertrait，只保留存储方法；`ArchiveService`、
   `GitService`、`ProcessService` 保持独立 trait。
2. `NativeExecutionContext` 改为三个字段：
   `{ storage: Arc<dyn Fs>, git: Arc<dyn GitService>, jobs: Arc<dyn ProcessService> }`，
   由 `RealFs` 的同一个 `Arc` 分别强转。这样以 `Arc<dyn Fs>` 传参的调用点签名全部不变
   （`Arc<RealFs>` → `Arc<dyn Fs>` 的强制转换仍然成立），需要 git 或 job 事件的地方改为
   取对应字段；`NativeExecutionContext` 的构造点会全部编译失败，那是预期的迁移清单。
3. 逐步把 `git_*` 与 `extract_archive` 的调用点从 `Fs` 迁到对应 trait；编译器报错
   即迁移清单。
4. `FakeFs` 同样拆成三份 impl；测试辅助不引入行为差异。
5. 删除 `trait Fs` 的 `ArchiveService` 与 `ProcessService` 绑定，确认没有任何地方依赖
   “一个对象同时能 git 和 trash”。

### 退出条件 {#phase-7-exit}

`rg 'trait Fs'` 无 supertrait；`CONV-EXP-012`（拆前后行为一致）PASS；
`cargo test -p fs -p project -p worktree` 全绿。

## 14. 阶段 8：remote 客户端 snapshot {#phase-8}

### 现状 {#phase-8-status}

`Worktree::vfs_snapshot()` 是 `self.as_local()?.vfs_snapshot.clone()`
（`crates/worktree/src/worktree.rs:894-895`），`vfs_authorizer()` 同样 `as_local()?`
（`:930-931`）。remote 只有 `vfs_root: VfsPath`（`:185`）。

### 工作 {#phase-8-work}

1. `RemoteWorktree` 持有 `vfs_snapshot: Option<VfsSnapshot>`，由
   `RemoteProviderProxy` 支撑；`try_remote` 之后异步挂载，挂载前保持 `None`。
2. `vfs_snapshot()` 对两种 variant 都返回 `Option`，去掉 `as_local()?` 前提。
3. `vfs_authorizer()` **保持** `as_local()?`（`:930-931`）不变，并补文档注释：它是
   “服务端把本地 worktree 暴露给远端对端”的策略，remote 客户端恒为 `None`。不要为
   remote 客户端伪造授权；客户端的每次请求由服务端 authorizer 判定。相应地把完成定义
   #9 的范围限定为 `vfs_snapshot()`。
4. 远端变更事件的精确身份：`UpdateWorktree` 的 `removed_entries`/`updated_entries` 与
   `PathChange` 仍以 `Arc<RelPath>` 为键（`crates/worktree/src/worktree.rs:763`、
   `:786-796`），不可表示的路径被 `.log_err()` 丢掉。阶段 5 改 `Event::UpdatedEntries`
   载荷后，这里改为按 `ResourceId` 生成变更并让远端非 UTF-8 条目落到
   `EntryIdentity::DisplayOnly`，不再丢条目。
5. `LocalWorktree` / `RemoteWorktree` 的共享字段抽成 `WorktreeVfsHandle`
   `{ snapshot: Option<VfsSnapshot>, root: VfsPath }`，减少重复。
6. 挂载任务必须存储或 detach；断线时 snapshot 保持可用但操作返回 `Disconnected`，
   重连后重建（`REMOTE_VFS_RECONNECT_DELAY` 已有退避基线，
   `crates/worktree/src/worktree.rs:91`）。
7. 集成测试：remote worktree 的 `vfs_snapshot()` 返回 `Some`；断线重连后
   `ResourceId` 语义仍然一致（同一路径重新解析得到同一 id，或明确失效）；远端非 UTF-8
   文件出现在面板中并可打开。

### 退出条件 {#phase-8-exit}

`CONV-EXP-013`（remote snapshot 一致性）PASS；`rg 'as_local\(\)\?'` 在
`crates/worktree/src/worktree.rs` 中仅剩 `vfs_authorizer()` 与与 VFS 无关的用途；远端
更新路径不再出现 `.log_err()` 丢弃条目。

## 15. 阶段 9：mount descriptor 持久化 {#phase-9}

### 现状 {#phase-9-status}

持久化只存 `ProviderPathV2`（`crates/project/src/project.rs:465-487`），不含
`MountId`。wire 上其实已有 `VfsPathV2 { mount_id, relative_path }`
（`crates/proto/proto/vfs.proto:52-55`），但持久化不用它。重启后 archive/remote buffer
的 mount 归属只能靠 worktree 推断。

### 工作 {#phase-9-work}

1. 新 sqlez domain `vfs_mounts`（放在 `crates/project/src/vfs_persistence.rs`）：
   `mount_key TEXT PRIMARY KEY`、`provider_id TEXT`、`path_encoding INTEGER`、
   `origin_json TEXT`、`created_at INTEGER`。迁移用 `ALTER`/`CREATE` 追加，不改历史
   step。
2. `MountKey` 按 D15 由 mount origin 确定性派生（SHA-256 取前 128 位，32 位小写十六
   进制），不是随机数；同一 origin 在每个会话得到同一个 key。`MountId` 仍是进程内
   `u64`。`VfsManager` 维护 `MountId ↔ MountKey` 双向映射，并在 `mount()` 时计算、
   在恢复时校验（重新派生后与持久化值不一致说明 origin 变了，按“mount 不可用”处理，
   不静默改写）。
3. 新 proto `PersistedVfsPathV2 { mount_key: bytes, path: ExactPathV2 }` 专供持久化，
   不改动 wire 上的 `VfsPathV2`。
4. editor 的 `editors.vfs_path` 与三个媒体 viewer 的 `provider_path` 改为存
   `PersistedVfsPathV2`；旧 `ProviderPathV2` 行在读取时推断 mount key 并立即重写，
   与既有迁移策略一致。
5. 启动恢复：先恢复 mount descriptor，再恢复 buffer；找不到 mount 的 buffer 明确报
   “mount 不可用”，不静默降级成原生路径。
6. 单测：往返编解码、旧格式迁移、mount 缺失时的错误路径。

### 退出条件 {#phase-9-exit}

`CONV-EXP-014`（持久化往返与旧格式迁移）PASS；重启后 archive buffer 能恢复到正确
mount；不存在只靠 `RelPath` 推断身份的持久化路径。

## 16. 阶段 10：`VfsUri` {#phase-10}

### 工作 {#phase-10-work}

1. 编码：`vfs://<mount_key>/<percent-encoded exact path>`。`/` 保留为分隔符，其余非
   unreserved 字节按 `%XX` 编码；不依赖 UTF-8。实现在新文件
   `crates/vfs/src/vfs_uri.rs`，字节编码用 workspace 已有的 `percent-encoding = "2.3.2"`
   （`Cargo.toml:685`），不需要新依赖。
2. `VfsUri` 本身只是字符串包装（解析出的 `mount_key` + 未解码的路径段）；把它变回
   `VfsPath` 需要 registry（`mount_key → MountId`），因此转换 API 挂在 manager 上：
   `VfsManager::encode_uri(&VfsPath) -> VfsResult<VfsUri>` 与
   `VfsManager::parse_uri(&VfsUri) -> VfsResult<VfsPath>`。不在 `VfsUri` 上放 `Display`
   以外的解析逻辑。
3. 人类可读形式单独提供 `VfsPath::display()`，不参与解析。
4. 用途：DAP `path` 字段、task 变量、`Copy Path` 变体、扩展设置里的资源引用。
5. 测试：POSIX byte name、Windows encoding、空路径、根路径、超长路径、`%` 字面量。

### 退出条件 {#phase-10-exit}

`CONV-EXP-015`（codec 往返）PASS；`VfsUri` 不出现在任何持久化列里（持久化仍用精确
proto）。

## 17. 阶段 11：扩展只读 FS API {#phase-11}

### 现状 {#phase-11-status}

扩展今天有三种文件访问：自己的 work dir（WASI 预开放，读写）、`download-file` 写
work dir、以及 worktree 资源的 `read-text-file`（`RelPath`，`crates/extension_api/wit/
since_v0.8.0/extension.wit:74`）。没有 VFS 访问。

### 工作 {#phase-11-work}

1. 在 `since_v0.8.0` 原地扩展（该版本尚未进入 Stable，Stable 上限是 0.6.0），新增
   `crates/extension_api/wit/since_v0.8.0/fs.wit`：
   `read-bytes(vfs-path: string, offset: u64, length: u64) -> result<list<u8>, error>`、
   `read-directory(vfs-path: string, cursor: option<u64>) -> result<dir-page, error>`、
   `stat(vfs-path: string) -> result<file-stat, error>`。
2. `crates/extension_api/wit/since_v0.8.0/extension.wit` 加 `import fs;`。两侧 bindgen
   都指向同一个目录（guest 侧 `crates/extension_api/src/extension_api.rs:365-367`，
   host 侧 `crates/extension_host/src/wasm_host/wit/since_v0_8_0.rs:43`），所以不需要
   维护第二份 WIT。
3. `crates/extension_host/src/wasm_host/wit/since_v0_8_0.rs` 实现 `fs::Host`。数据通过
   新 delegate trait 注入（照 `crates/extension/src/extension.rs:38-60` 的
   `WorktreeDelegate`/`ProjectDelegate`/`KeyValueStoreDelegate` 写法），由
   `crates/project` 实现，`extension_host` 不直接依赖 `vfs`。
4. 新 capability `ExtensionCapability::ReadFiles(ReadFilesCapability)`，manifest 键
   为 `fs:read`。需要改的文件：
   - 新建 `crates/extension/src/capabilities/read_files_capability.rs`，并在
     `crates/extension/src/capabilities.rs` 加 `mod`/`pub use` 与枚举变体；
   - `crates/extension/src/extension_manifest.rs:162` 增加 `allow_read_files`；
   - `crates/extension_host/src/capability_granter.rs:65` 增加 `grant_read_files`；
   - `crates/extension_host/src/extension_settings.rs:50-60` 让授权出现在设置页。
5. 授权：每次调用先过 capability，再用 mount 的 `VfsAuthorizer` 判定；越界路径返回
   `PermissionDenied`，错误 detail 不含 native path。
6. `crates/extension_api/src/fs.rs` 提供 guest shim；`extensions/test-extension` 增加
   覆盖“允许路径成功、越界路径失败”的用例。
7. 文档：`docs/src/extensions/capabilities.md` 增加 `fs:read` 说明（这是扩展作者可见的
   权限声明，属于用户文档）。

### 退出条件 {#phase-11-exit}

`CONV-EXP-016`（扩展沙箱）PASS；未授权扩展读取越界路径失败且无 native path 泄漏；
`extensions/test-extension` 能通过新 API 读取工作区文件。

## 18. 阶段 12：写入语义补全 {#phase-12}

### 工作 {#phase-12-work}

1. `VfsProvider` 增加两个带默认实现（返回 `Unsupported`）的方法：`set_times` 与
   `create_hard_link`。两者当前都不存在；trait 现有的写方法只有 `set_permissions`、
   `remove`、`rename`、`copy`、`read_link`（`crates/vfs/src/provider.rs:790-870`）。
2. capability 不复用 `links.hard_links`：那个字段的既有语义是“能**区分并报告** hard
   link”（文档注释见 `crates/vfs/src/provider.rs:126-132`），与“能**创建** hard link”
   是两件事，混用会让 `Unsupported` 的含义含糊。改为在 `MutationCapabilities` 增加
   `set_times: SupportLevel` 与 `create_hard_link: SupportLevel`，wire 侧在
   `VfsMutationCapabilitiesV2` 追加编号。
3. `LocalProvider` 实现 `set_times` 与 `create_hard_link`（对应 capability 升为
   `Native`）。`links.hard_links` 保持现有取值不变：升级“能区分 hard link”是独立判断，
   不在本阶段范围。
4. `stat_many` 在 `LocalProvider` 覆盖为批量实现，消费 `maximum_stat_batch`。默认实现
   已经存在，`directories.stat_many` 目前报 `Emulated`
   （`crates/fs/src/vfs_provider.rs:1705-1709`），完成后升为 `Native`。
5. 原子保存：`write_rope_to_provider` 在 `mutations.maximum_rename_atomicity` 达到
   `AtomicWithinDirectory` 时改为 temp 文件 + rename（`LocalProvider` 已报该级别，
   `crates/fs/src/vfs_provider.rs:1714`）；否则保持直接写入并如实报告 `BestEffort`。
6. 崩溃安全测试：保存中途注入失败，目标文件要么是旧内容，要么是新内容，不出现截断。

### 退出条件 {#phase-12-exit}

`CONV-EXP-018`（原子保存崩溃安全）PASS；新增 capability 在 UI 与 consumer 中被消费，
不是只声明。

## 19. 阶段 13：remote 到 remote 直传 {#phase-13}

### 现状 {#phase-13-status}

跨 mount 传输一律经客户端中转（`crates/vfs/src/transfer.rs:406-681`）；只有同一
provider 实例走服务端原生 `copy`/`rename`（`:284-288`）。两个 mount 即使在同一台远端
也会经客户端。

### 工作 {#phase-13-work}

1. 先做不依赖新 wire 的判定：`RemoteProviderProxy` 持有 `Arc<dyn RemoteVfsTransport>`
   （`crates/rpc/src/vfs.rs:1929-1936`），给 `RemoteVfsTransport` 加
   `fn host_identity(&self) -> Option<HostIdentity>`，由同一个 client/连接实现返回同一
   值。同一连接上的两个 mount 必然由同一 server 进程服务，这是最可靠的同源判定。
2. 跨连接时再走 wire：`VfsNegotiateResponseV2` 增加 `string host_id = 6;`
   （现有字段 1–5 见 `crates/proto/proto/vfs.proto:326-332`），由
   `crates/remote_server/src/headless_project.rs:448` 的 handler 填入进程级稳定值；
   `crates/rpc/src/vfs.rs` 两侧转换补齐。`host_id` 只用于同源判定，不参与授权。
3. `VfsManager` 增加可注入的 `TransferRouter`；当 source 与 target 的 host identity 相同
   且该 host 暴露 `server_transfer` capability 时，改发一条双 mount 的
   `VfsTransferRequestV2`（新 message，需同时定义 request/response 与进度事件）。
4. 服务端 handler 用两个本地 snapshot 执行同一套 `transfer_same_provider` /
   `transfer_cross_provider` 逻辑，保留 SHA-256 验证与 staging rename。
5. 客户端只接收进度与结果，不接收字节。
6. 拒绝条件（满足任一即记 REJECTED，保留中转路径并附证据）：
   - 无法在不放宽 `VfsAuthorizer` 的前提下同时授权两个 mount；
   - 服务端无法按 mount id 解析出第二个 snapshot（例如目标 mount 属于另一个 client
     session）；
   - `CONV-EXP-017` 显示客户端内存增量没有下降。

### 退出条件 {#phase-13-exit}

`CONV-EXP-017` PASS 或 REJECTED 并附证据；若实施，客户端内存增量与网络字节显著低于
中转路径。

## 20. 阶段 14：新 provider {#phase-14}

### 工作 {#phase-14-work}

1. 必做 `TarProvider`：复用 `ArchiveProvider` 的只读、限额、惰性模板（`ArchiveProvider`
   目前只支持 ZIP/Zip64，`crates/vfs/src/archive_provider.rs:164`）。tar 读取用
   workspace 已有的 `async-tar = "0.6.1"`（`Cargo.toml:502`，`crates/fs` 已在用），
   不需要新依赖，也不需要 ADR。验证同一份 corpus 在 local 与 remote mount 上产生相同树。
2. 条件项 `GitTreeProvider`：以 Git 对象库为只读 provider，用于 diff/review 的树视图；
   需要 ADR 确认 `watch: Unsupported` 与无 native path 的 UI 行为，并给出“不实施”的
   判据（例如 diff 视图已能用现有 `git` 接口满足）。
3. 条件项 `SftpProvider` 或 `S3Provider`：先写 ADR（版本、license、feature set、
   runtime 影响、替代方案），再实施；无 ADR 不得合并。
4. provider conformance 套件：把 `crates/vfs` 现有测试中可复用的契约断言抽成
   `test_support.rs` 的 conformance runner，新 provider 必须跑通同一套断言。

### 退出条件 {#phase-14-exit}

`CONV-EXP-019`（provider conformance）对 `TarProvider` PASS；`GitTreeProvider` 与
`Sftp`/`S3` 有明确的实施或 REJECTED 结论及证据；`ReadCapabilities.stream` 等新方法在
conformance 中有覆盖。

## 21. 阶段 15：增量索引 {#phase-15}

### 工作 {#phase-15-work}

1. 直接消费已有类型，不新增并行结构：`SnapshotDelta { changes: Vec<ResourceChange> }`
   （`crates/vfs/src/snapshot.rs:83-86`）、
   `ResourceChange { resource_id, path, kind }`（`:76-81`）与
   `ResourceChangeKind { Created, Modified, Removed, Renamed { old_path } }`
   （`:69-74`）。阶段 4 已经产出 delta，本阶段只补 consumer 需要的查询辅助（按
   `resource_id` 过滤、按 `kind` 分组），不定义 `changed_paths`/`removed_paths` 这类重复
   类型。
2. `crates/search` 消费 delta 更新索引，替代全量重扫：`Created`/`Modified` 重索单个文件，
   `Removed` 按 `resource_id` 删条目，`Renamed { old_path }` 迁移已有条目（保留分数与
   历史）。无 delta 或 `SnapshotStats.rescan_state` 表示重建时回退全量并记录原因。
3. 先验证 `reconcile_directory`/`load_directory` 是否真的发出 `Renamed` 而不是
   `Removed` + `Created`；若目前是后者，本阶段补上 rename 识别，或在账本记为已知退化
   并接受删了重建的成本。
4. 符号索引与 buffer 跟随 rename：消费者按 `resource_id` 迁移，不按 `RelPath` 匹配，
   避免 lossy 路径把两个条目混在一起（阶段 5 的 `Event::UpdatedEntries` 改动是前提）。
5. 断言：单文件修改不触发全树重扫（用计数 provider 断言 `read_directory` 调用次数）。

### 退出条件 {#phase-15-exit}

`CONV-EXP-020`（delta 驱动新鲜度）PASS；单文件保存的索引更新成本与工作区规模解耦。

## 22. 阶段 16：收敛与验收 {#phase-16}

### 工作 {#phase-16-work}

1. 删除兼容层：`UpdateWorktree` 快照推送（若阶段 4 已完全替代）、`ProviderPathV2` 旧
   持久化读取分支（阶段 9 迁移窗口结束）、阶段 6 之前的旧动作判断、`terminal_mode`
   的 mount id 偏移（若阶段 3 有遗留）。
2. 每个 compatibility adapter 在账本中有“调用点为 0”的证据后再删除。
3. 全量验证：

```sh
cargo fmt --all -- --check
cargo test --workspace
./script/clippy
./script/check-philosophy
./script/check-todos
cd docs && npx prettier --check src/
mdbook build docs
```

4. proto 检查：若本机无 `buf`，写 runbook 并标记 NOT RUN，不得冒充 PASS。
5. 跨平台验证：Windows/macOS 的 case 行为、非 UTF-8（仅 Unix）、remote transport
   组合，能为则跑，不能则 runbook + NOT RUN。
6. 账本收尾：每阶段 SHA、实验结论、剩余风险、移交说明。
7. 合并：全部完成定义满足后，请用户确认，再把 `vfs-convergence` 合并回 `main`
   （默认 `--no-ff` 保留阶段边界），不 push。

### 退出条件 {#phase-16-exit}

完成定义 1 至 18 全部满足，或逐条有明确记录的不适用理由。

## 23. 实验合同 {#experiments}

实验记录写入[账本](./progress.md)，不复用前序 `VFS-EXP-*` 编号。

| ID           | 目的                   | 方法                                       | 通过阈值                             |
| ------------ | ---------------------- | ------------------------------------------ | ------------------------------------ |
| CONV-EXP-001 | 基线复现               | 六条垂直链计时与计数                       | 同机两次结果同量级                   |
| CONV-EXP-002 | 可观测性与可复现性     | 打开 debug 日志跑两次同规模扫描并比对计数  | 每操作恰好一个 span；两次计数一致    |
| CONV-EXP-003 | 预算摊销               | 250k 条目目录 + 100k identities            | intern p95 随 N 近线性               |
| CONV-EXP-004 | 分页成本               | 100k 目录取第 1/5/25 页                    | 第 25 页成本 < 2× 第 1 页            |
| CONV-EXP-005 | 整文件内存             | 8 GiB 稀疏文件读取                         | 返回 `TooLarge`，RSS 增量 < 100 MiB  |
| CONV-EXP-006 | watch 缺口与恢复       | 注入丢批 + 断线 30s + 1k 变更              | 缺口被检测并触发重建，终态一致       |
| CONV-EXP-007 | 非 UTF-8 端到端        | Unix 上 `\xff.rs` 扫描、面板、打开、重命名 | 无静默丢弃、无 panic、操作成功       |
| CONV-EXP-008 | mount 吊销             | unmount 后使用旧句柄                       | 全部返回 `NotFound`                  |
| CONV-EXP-009 | case collision         | macOS/Windows 上 `foo.rs`/`Foo.rs`         | `NameCollision` typed error          |
| CONV-EXP-010 | capability × action    | 对 `Unsupported` 操作枚举 UI 入口          | 无不可用入口、无 silent no-op        |
| CONV-EXP-011 | 错误码分类与文案       | 逐类触发 TooLarge/PermissionDenied/…       | 有 typed 分类与文案，无字符串匹配    |
| CONV-EXP-012 | `Fs` 拆分无回归        | 拆分前后同一测试集                         | 行为差异为 0                         |
| CONV-EXP-013 | remote snapshot 一致性 | 断线重连后解析同一路径                     | 身份一致或明确失效语义               |
| CONV-EXP-014 | 持久化往返             | 重启后恢复 archive/remote buffer           | mount 归属正确，旧格式迁移成功       |
| CONV-EXP-015 | `VfsUri` 往返          | byte name / Windows encoding / 边界        | 编解码完全相等                       |
| CONV-EXP-016 | 扩展沙箱               | 恶意扩展读 mount 外路径                    | `PermissionDenied`，无 native path   |
| CONV-EXP-017 | remote 直传            | 1 GiB 跨 mount 传输对比                    | 客户端内存增量 < 64 MiB，或 REJECTED |
| CONV-EXP-018 | 原子保存               | 保存中途注入失败                           | 目标文件非旧即新，无截断             |
| CONV-EXP-019 | provider conformance   | Tar/GitTree 跑同一套断言                   | 全部断言通过                         |
| CONV-EXP-020 | delta 驱动索引         | 单文件保存后统计 `read_directory` 次数     | 不触发全树重扫                       |

## 24. 度量与门禁 {#metrics}

以下命令必须可复制且结果写入账本。前两条是“必须归零”的硬门禁，后三条是单调性检查，
不是绝对目标。

```sh
# 硬门禁：mount id 分配者唯一，crates/vfs 之外必须为 0（测试模块需注明理由）
rg -n 'MountId::new' crates --glob '*.rs' | rg -v 'crates/vfs/src/manager.rs'

# 硬门禁：扫描层不得再有 UTF-8 往返与静默丢弃，必须为 0
rg -n 'to_str\(\)|provider_path_to_legacy_utf8|\.log_err\(\)' crates/worktree/src/worktree.rs

# 单调性：旧的手写 capability/错误判断必须逐阶段下降，阶段 16 归零
rg -n 'supports_trash_restore|contains\("File is too large' crates --glob '*.rs'

# 单调性：可观测性下界，只增不减
rg -c 'tracing::' crates/vfs/src/*.rs

# 单调性：上帝接口，阶段 7 之后 supertrait 必须为 0
rg -n 'pub trait Fs' crates/fs/src/fs.rs
```

阶段 16 汇总时逐条给出数值与判定（PASS 或带理由的例外）。`log_err()` 在 worktree 中
有与 VFS 无关的合法用途（例如日志型回退），阶段 5、8 收尾时必须逐个说明保留原因。

## 25. 与旧计划的关系 {#relationship}

- 旧计划的 Phase 0 至 Phase 8 已经合并，其 ADR 仍然有效，本文不推翻。
- 旧计划完成定义 #2、#3、#4、#5、#11、#14、#15 未真正满足；本文的阶段 5、6、7、9
  承接这些条目，完成定义逐条对应见第 2 节。
- 旧账本归档后仍可查历史决策；新账本只记录本文的阶段。
- 若两者冲突，以本文为准，并在账本记录冲突原因。
