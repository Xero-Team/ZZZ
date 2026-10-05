---
title: VFS Refactoring Execution Plan
description: Staged execution contract for replacing ZZZ's path, local, remote, and archive filesystem architecture.
---

# VFS 完整重构执行计划

本文把[架构研究报告](./report.md)转换为可以由长期 Goal 连续执行、逐阶段验收和
随时恢复的实施合同。报告解释目标架构；本文规定顺序、交付物、验证门槛和完成
定义。

- 计划基线：ZZZ `6500fbdeccd7d523acfc69161d6371b631fdb6ac`
- 架构合同：[report.md](./report.md)
- 实验合同：[experiments.tsv](./experiments.tsv)
- 执行账本：[../vfs-refactor-progress.md](../vfs-refactor-progress.md)
- 可复制 Goal：[goal.md](./goal.md)

如果执行开始时 HEAD 已变化，先检查覆盖范围内的新提交。只要用户工作可以保留，
就记录新 baseline 并继续。不得 reset、clean、checkout 覆盖或丢弃现有修改。

## 1. 目标与完成定义 {#goal-and-done}

目标是让 ZZZ 通过一套无损路径、统一 provider、稳定 snapshot 和版本化 remote
protocol 访问本地、远程和虚拟资源。ZIP 作为第一个 composable provider 验证架构。

只有同时满足以下条件，整个 Goal 才能标记完成：

1. 阶段 0 至阶段 10 的必做项全部完成；所有条件阶段有“实施”或“拒绝”的明确
   结论。
2. `Fs` 不再同时拥有 storage、Git、archive extraction 和 job reporting；旧 trait
   已删除或只剩有明确移除期限的窄 compatibility façade。
3. `Worktree` 普通文件操作不再通过 `Local`/`Remote` 分支决定实现；local、remote
   和 archive 都通过 provider dispatch。
4. Project Panel、Workspace、Buffer、Search 和媒体查看器以 `ResourceId`/
   `VfsPath` 为资源身份，不要求绝对 `PathBuf`。
5. native path wire format 无损支持 POSIX byte names 与 Windows path encoding；旧
   protobuf string path 已删除，或只在经过批准的版本兼容窗口保留。
6. `VfsFile` 支持 positioned I/O；local 和 remote provider 对并发、取消、EOF、
   sparse file 和 large file 的语义一致。
7. `VfsSnapshot` 提供 stable session identity、惰性 directory state、版本、缓存
   上限、watch sequence、overflow 和 deterministic rescan。
8. remote protocol 支持 capability/path negotiation、分页目录、positioned I/O、
   cancellation、idempotency、expected version、watch resume 和 reconnect。
9. Git、LSP、terminal、task 和 debugger 通过明确的 native execution/mapping
   边界工作；archive/memory 等无 native mapping 的资源不会伪装成物理文件。
10. `ArchiveProvider` 可对同一测试 corpus 在 local 与 remote mount 上产生相同树和
    member bytes；远程 listing 不传输整个 archive。
11. unsupported 操作由 capability 和 typed error 表达，不存在已知 silent no-op。
12. `VFS-EXP-001` 至 `VFS-EXP-014` 的必做实验全部 `PASS`；只有计划明确标记为
    条件项的设计可以 `REJECTED`；当前主机无法执行的平台 QA 可以保留有 runbook
    的 `NOT RUN`。任何未解决的 `FAIL` 都不满足完成定义。
13. 受影响的 targeted tests、`./script/clippy`、`cargo test --workspace`、
    `./script/check-philosophy`、proto lint/format、mdBook build 和 docs Prettier
    在本机或明确的 CI 运行中通过，或只剩记录并从 clean baseline 复现的既有失败。
14. 执行账本记录每阶段 baseline、改动、实验、验证、提交 SHA、剩余风险和下一步。
15. 没有未说明的 TODO、临时 feature、双实现、死 compatibility path、无 owner
    后台任务或未处理 fallible operation。

## 2. 范围 {#scope}

核心范围：

- `crates/fs`
- `crates/worktree`
- `crates/project`
- `crates/workspace`
- `crates/editor`
- `crates/proto`
- `crates/rpc`
- `crates/remote`
- `crates/zzz`
- 新增的 VFS 和 archive logical components

按 consumer 迁移需要修改：

- `crates/language`
- `crates/lsp`
- `crates/git`
- `crates/search`
- `crates/project_panel`
- `crates/image_viewer`
- `crates/pdf_viewer`
- `crates/audio_viewer`
- `crates/video_viewer`
- `crates/typst_preview`
- persistence 与 settings 相关 crate

不属于本次目标：

- 更换 GPUI、Smol 或全局 async runtime；
- FUSE、WinFSP 或 OS-level mount；
- 可写 ZIP；
- 无需求的 object-storage provider；
- 重写 Project Panel 或 Editor 视觉设计；
- 恢复账号、遥测、托管协作、默认网络或被 ZZZ 删除的商业表面。

## 3. 执行规则 {#execution-rules}

### 3.1 工作区与提交 {#workspace-and-commits}

1. 开始代码修改前记录 `git status`、HEAD、分支和现有 untracked/modified files。
2. 保留所有用户修改。不得运行 destructive reset、clean 或覆盖命令。
3. 在专用分支或隔离 worktree 工作。不得 push、开 PR 或合并 `main`，除非用户在
   当前对话明确要求。
4. 持续更新 `docs/src/development/vfs-refactor-progress.md`。每个阶段记录：
   baseline、设计决策、改动、实验、验证命令、PASS/FAIL/REJECTED/BLOCKED/NOT RUN、
   提交 SHA 和下一步。
5. 原始 benchmark、fault injection output、large fixtures 和临时数据放在
   `.tmp/vfs-refactor/`。只把可复现命令、摘要和结论写入 tracked docs。
6. 每个提交只包含一个可独立审查和回退的结构或行为变化，并使用
   `git commit -s`。阶段结束前不留下不编译的提交。
7. 不为追求小 diff 保留错误边界。允许大面积重构，但每一步必须有 compatibility
   seam、tests 和清晰 owner。
8. 每个阶段结束时使用 `hunting-code-smells` 审查 touched diff，修复确认的代码
   异味后再验收。若移植 Zed upstream commit，必须先使用 `absorbing-upstream`
   做 A/B/C 分类；不得把普通原创重构伪装成 upstream absorption。

### 3.2 代码规范 {#code-standards}

- 遵守根 `AGENTS.md`、`.rules`、`clippy.toml` 和触及 crate 的局部规则。
- 不新增 `unwrap()`；避免可能 panic 的 indexing。
- 不用 `let _ =` 丢弃 fallible operation。传播、记录或显式处理错误。
- 不创建 `mod.rs`；新 crate 使用描述性 library root path。
- 使用完整变量名，不使用无意义缩写。
- `Entity::update` 和 `update_in` closure 使用内部 `cx`，不得重入同一 entity。
- 所有 `cx.spawn`、`cx.background_spawn` Task 必须 await、detach 或存储。
- GPUI tests 使用 executor timer，不使用 `smol::Timer::after`。
- 用户可见错误必须 i18n，并同步 `en.json` 与 `zh-CN.json`。
- 默认构建不启用 benchmark、trace、journal 或 fault-injection 开销。
- 公共类型先写 observable contract，再写实现；不要把 provider-specific condition
  泄漏给 consumer。
- 错误必须保留 operation、mount 和 source chain，但不得记录 credential 或 private
  content。

### 3.3 架构不变量 {#architecture-invariants}

以下不变量贯穿所有阶段：

1. path identity 与 display string 分离。
2. consumer 不根据 local/remote/archive 选择文件操作实现。
3. provider capability 与授权分离；capability 不替代权限检查。
4. watcher event 是 hint，snapshot reconciliation 是真相。
5. remote operation 可取消；断线不会遗留不可回收 handle 或未完成 write。
6. write、rename 和 delete 不虚构 atomicity；无法保证时返回明确语义。
7. cross-provider move 只有在 copy 验证成功后才允许 delete source。
8. native path 只在 resource 所在 execution host 暴露。
9. code analysis 只消费 content/snapshot delta，不执行 provider I/O。
10. archive provider 只读，默认不 materialize、不执行 entry、不跟随 symlink。
11. cache 有容量、版本和 eviction；stale cache 不作为成功结果静默返回。
12. compatibility adapter 有调用点清单、删除阶段和 tests。

### 3.4 第三方依赖 {#third-party-dependencies}

- `typed-path` 可用于 lexical path operations，但必须包在 ZZZ-owned types 后面；
  wire codec 和 identity 不由依赖定义。
- `cap-std` 只能在实际通过 provider conformance 和平台测试后用于 local/subtree
  containment。
- `async_zip` 用于 `ArchiveProvider`。
- `notify` 保留为 LocalProvider watcher backend。
- OpenDAL/remotefs 不进入核心 API；只有具体 optional provider workload 批准后添加。
- 不引入 `rust-vfs` 或 Wasmer `virtual-fs` 作为 VFS 基础。
- 新依赖必须记录版本、license、feature set、runtime 影响和替代方案。

## 4. 阶段总览 {#phase-overview}

| 阶段 | 结果                                               | 进入条件         | 退出条件                           |
| ---- | -------------------------------------------------- | ---------------- | ---------------------------------- |
| 0    | baseline、ADR、实验 harness                        | 工作树可安全继续 | 当前行为可复现，合同冻结           |
| 1    | 无损 path/resource types 与 v2 wire schema         | 阶段 0           | VFS-EXP-002 通过                   |
| 2    | provider contract、LocalProvider、职责拆分         | 阶段 1           | VFS-EXP-003/004/005 通过           |
| 3    | VfsSnapshot、stable ResourceId、Worktree 接入      | 阶段 2           | VFS-EXP-006 通过                   |
| 4    | RemoteProviderProxy 与统一 VFS RPC                 | 阶段 2/3         | VFS-EXP-007/008 通过               |
| 5    | Project/Workspace/Editor/媒体/Search consumer 迁移 | 阶段 3/4         | VFS-EXP-009 通过                   |
| 6    | LSP/Git/Tasks/native execution 边界                | 阶段 5           | VFS-EXP-010 通过                   |
| 7    | ArchiveProvider 与 ZIP UI 集成                     | 阶段 4/5         | VFS-EXP-011 通过                   |
| 8    | Subtree/ReadOnly/Cache 与 overlay 决策             | 阶段 3/7         | VFS-EXP-012 通过；overlay 单独决策 |
| 9    | cross-provider operations 与旧 wire 移除           | 阶段 4-8         | VFS-EXP-013 通过                   |
| 10   | 收敛、全量验证、文档和移交                         | 前述阶段完成     | 完成定义全部满足                   |

## 5. 阶段 0：baseline 与合同 {#phase-0}

### 工作 {#phase-0-work}

1. 记录 HEAD、Rust toolchain、OS、filesystem、case behavior、remote transport 和测试
   工作区大小。
2. 对以下垂直链建立 baseline：
   - local initial scan、expand、watch；
   - open text、binary 和 media；
   - save、save as、rename、copy、trash、restore；
   - remote scan、open、download、reconnect；
   - Git/LSP resource operations；
   - current path persistence。
3. 运行并记录：

```sh
cargo check --locked -p fs -p worktree -p project -p workspace -p editor
cargo test --locked -p fs -p worktree -p project
cargo test --locked -p workspace -p editor
./script/clippy -p fs -p worktree -p project -p workspace -p editor
```

4. 建立 ADR，至少覆盖：
   - path encoding 与 display；
   - `ResourceId` lifetime；
   - provider capability；
   - positioned I/O；
   - snapshot freshness；
   - event ordering/overflow；
   - remote retry/idempotency；
   - symlink 与 containment；
   - atomicity 与 expected version；
   - LSP/native mapping；
   - archive limits。
5. 建立 provider conformance harness、fault injection transport 和 path corpus。
6. 更新 `VFS-EXP-001` 的 baseline 与所有实验中尚未固定的数值预算。

### 退出条件 {#phase-0-exit}

- baseline failures 有复现命令和分类；
- ADR 没有影响 Phase 1 public types 的未决问题；
- conformance harness 能运行 memory/legacy provider；
- fixtures 和随机 seed 固定。

## 6. 阶段 1：path、resource identity 与 wire schema {#phase-1}

### 工作 {#phase-1-work}

1. 新建内聚的 VFS logical component。默认优先一个 `crates/vfs` crate，而不是在未
   证明依赖环前拆成多个微型 crate。
2. 定义：
   - `MountId`
   - `ResourceId`
   - `VfsPath`
   - `ProviderPath`
   - `NativePath`
   - `PathEncoding`
   - exact component、display component 和 lookup key
3. 定义 safe join、parent、strip prefix、comparison 和 serialization。不允许通过
   `to_string_lossy()` 参与 identity、hash、lookup 或 RPC。
4. 在 `crates/proto/proto/vfs.proto` 增加 v2 path/resource messages。旧 string fields
   暂时保留以兼容当前客户端，但新代码内部只使用 exact representation。
5. 为旧 `RelPath`、`ProjectPath` 和 protobuf string path 建立显式 adapter，记录所有
   caller；adapter 遇到不可表示名称必须返回 typed error，不能丢失后继续。
6. 对 POSIX bytes、Windows drive/UNC/verbatim、separator、case 和 archive raw names
   添加 unit/property tests。

### 退出条件 {#phase-1-exit}

- `VFS-EXP-002` 通过；
- wire round trip 不依赖运行客户端的 OS；
- public consumer 尚未强制迁移，但所有新 VFS code 禁止使用 string identity。

## 7. 阶段 2：Provider 与 LocalProvider {#phase-2}

### 2A. Provider contract {#phase-2-provider}

1. 实现报告定义的 `VfsProvider`、`VfsFile`、metadata、capability 和 error model。
2. `read_dir` 使用 page/cursor，不返回无界 `Vec`。
3. file primitive 使用 `read_at`/`write_at`。提供 cursor adapter 兼容 decoder。
4. operation options 包含 cancellation、expected version、overwrite 和 atomicity。
5. `LegacyFsProvider` 包装旧 `Fs`，让 consumer 可分阶段迁移。

### 2B. LocalProvider {#phase-2-local}

1. 使用 native positioned I/O：Unix `FileExt::read_at`，Windows `seek_read`。
2. 保留当前 watcher 平台修复，但输出统一 `EventBatch`。
3. watcher 启动早于 initial scan；overflow 产生显式事件。
4. 读取 exact directory names，不经过 Unicode string round trip。
5. 实现 root containment、symlink metadata、case behavior 和 provider file key。
6. 在后台线程执行 blocking native I/O，不阻塞 GPUI foreground。

### 2C. 拆分旧 `Fs` {#phase-2-split-fs}

1. 将 Git operations 迁到 `GitService`。
2. 将 job/process reporting 迁到 process/service owner。
3. 将 archive extraction 迁到 archive utility/provider。
4. 保留必要 façade，但不允许新增 caller。

### 退出条件 {#phase-2-exit}

- `VFS-EXP-003`、`VFS-EXP-004`、`VFS-EXP-005` 通过；
- Fake/Memory/Legacy/Local provider 运行同一 conformance suite；
- targeted checks/tests/clippy 通过；
- `Fs` 调用点数量开始单调下降，并记录在进度账本。

## 8. 阶段 3：Snapshot 与 Worktree {#phase-3}

### 工作 {#phase-3-work}

1. 实现 `VfsSnapshot`/`ResourceRegistry`：
   - stable session `ResourceId`；
   - current path 与 provider file key；
   - lazy children/metadata；
   - version、fresh/stale/loading/tombstone；
   - bounded cache 与 eviction；
   - event cursor 与 rescan state。
2. `ResourceId` rename 后保持稳定，仅在 provider 无法证明 rename 时允许
   delete/create identity。
3. Worktree scanner 改为 provider-independent snapshot consumer。
4. 把 ignore/private/git status 作为 Worktree/Project policy，不塞进 provider
   metadata。
5. Project Panel 先通过 compatibility view 读取 snapshot，保持 UI 行为。
6. 对目录展开、rename storm、delete/recreate、case collision、symlink 和 overflow
   建立 deterministic tests。

### 退出条件 {#phase-3-exit}

- `VFS-EXP-006` 通过；
- unexpanded directory 不 materialize 全部 descendants；
- snapshot diff 可重放且结果确定；
- Worktree 普通 scan path 不再直接依赖 `RealFs`。

## 9. 阶段 4：统一远程 VFS {#phase-4}

### 工作 {#phase-4-work}

1. 服务端运行同一 `VfsManager`、`VfsSnapshot` 与 LocalProvider。
2. 客户端实现 `RemoteProviderProxy`，provider API 与 local 完全相同。
3. proto 支持：
   - mount/capability/path negotiation；
   - `stat_many`、paged `read_dir`；
   - open/close handle、positioned I/O；
   - bounded streams、backpressure、cancellation；
   - operation ID、idempotency、expected version；
   - watch subscribe/resume/overflow；
   - reconnect 与 protocol version。
4. handle 有 lease/close；连接关闭和 Task cancellation 必须释放。
5. remote write 在 commit/response 丢失时可判定是否已提交，不能盲目重放。
6. server 重新验证 root、path、private file 和 authorization。
7. 建立 loopback transport 与 fault injection，不依赖真实 SSH 才能测失败路径。

### 退出条件 {#phase-4-exit}

- `VFS-EXP-007`、`VFS-EXP-008` 通过；
- reconnect 后 snapshot 确定收敛；
- remote listing 和 range read 不收集 whole file；
- 新 remote feature 不再增加专用字节传输 RPC。

## 10. 阶段 5：Consumer 迁移 {#phase-5}

按以下顺序迁移，每一组独立提交并删除不再需要的 branch：

1. `language::File` 与 BufferStore；
2. Workspace item identity 与 persistence；
3. Project Panel、file finder、reveal 和 drag/drop；
4. project search、path matching 与 diagnostics map；
5. image/PDF/audio/video/Typst 和其它 binary consumers；
6. settings location、trusted worktrees 和 private path policy。

规则：

- consumer 使用 `ResourceId`/`VfsPath`；
- 只有需要 OS integration 的 action 请求 native mapping；
- save/reload 通过 versioned provider operation；
- read-only capability 控制 UI action；
- persistence 存 mount descriptor 与 resource path，不存临时 runtime ID；
- local/remote UI 行为必须共享 tests。

### 退出条件 {#phase-5-exit}

- `VFS-EXP-009` 通过；
- ordinary consumer 不检查 `is_local()` 或匹配 `Worktree::Local/Remote`；
- image/download 等 whole-file 专用 RPC 有删除清单并开始移除；
- workspace restore 能处理 missing/disconnected mount，不 panic。

## 11. 阶段 6：LSP、Git 与 native execution {#phase-6}

### 工作 {#phase-6-work}

1. 引入 `NativeExecutionContext`，只在 resource 所在 host 暴露 native path、process、
   Git 和 terminal 能力。
2. 引入 `LspPathMapper`：
   - VFS resource → execution-host file URI；
   - LSP URI → VFS resource；
   - workspace edit 预验证；
   - read-only/unmappable resource 明确拒绝。
3. GitStore 不依赖通用 provider trait 中的 Git 方法。
4. remote LSP/Git 在 remote server 内执行；客户端不把 remote path 解释为本地
   `PathBuf`。
5. virtual document 如需 mirror，定义 version、cleanup、privacy 和 reverse edit
   policy；默认不 materialize。

### 退出条件 {#phase-6-exit}

- `VFS-EXP-010` 通过；
- resource operation edit 不会部分应用后才发现 unsupported；
- Git/LSP/Task tests 覆盖 local 与 loopback remote；
- base VFS trait 不含 Git 或 command API。

## 12. 阶段 7：ArchiveProvider 与 ZIP {#phase-7}

### 工作 {#phase-7-work}

1. `ArchiveProvider` 包装 `VfsFile::read_at`，不依赖本地 absolute path。
2. 使用 Central Directory 建立只读 tree；UI list virtualization 与 provider lazy
   content 解压分离。
3. remote archive 在服务端 mount，客户端不下载 whole archive。
4. 支持 ZIP、Zip64，以及 `.jar`、`.war`、`.apk`、`.aab`、`.whl`、`.vsix`。
5. Office/EPUB 等格式默认通过“Open as Archive”显式进入，避免覆盖未来专用 viewer。
6. member text 使用 read-only Buffer 和 filename-based language detection；没有 native
   mapping 时不注册 LSP。
7. image member 从 bytes 渲染，不制造假本地 `worktree::File`。
8. 处理 duplicate name、raw name、traversal、encrypted entry、unsupported compression、
   CRC、corruption、bomb 和 nested depth。
9. 外层 resource version 变化时，使 child mount stale 并安全 reload。

### 退出条件 {#phase-7-exit}

- `VFS-EXP-011` 通过；
- local/remote archive tree 与 member bytes 完全一致；
- listing 不解压 member，不产生磁盘垃圾；
- 所有安全 limit 有设置、默认值、错误和 tests。

## 13. 阶段 8：Composition layers {#phase-8}

### 必做 {#phase-8-required}

- `MemoryProvider`
- `SubtreeProvider`
- `ReadOnlyProvider`
- `CacheProvider`

每个 wrapper 必须重新计算 capability，不能暴露 inner provider 已被 wrapper 禁止的
操作。Cache key 包含 mount、exact path、range 和 source version。

### Overlay 决策 {#phase-8-overlay}

先写 specification 与 model tests，再实现：

- precedence；
- directory merge；
- whiteout storage；
- lower-only file mutation；
- rename across layers；
- crash consistency；
- source provider disconnect；
- watcher event translation。

如果 `VFS-EXP-012` 无法证明确定语义，删除 overlay prototype，记录
`REJECTED`，继续完成其它阶段。不得为了架构完整感保留不可靠 overlay。

### 退出条件 {#phase-8-exit}

- Memory/Subtree/ReadOnly/Cache 的 conformance 与 capability composition 通过；
- `VFS-EXP-012` 的必做部分为 `PASS`；
- overlay 在实验的 `Decision` 中单独记录 `PASS` 或 `REJECTED`，不能用整个实验的
  `REJECTED` 掩盖必做 provider 未通过。

## 14. 阶段 9：跨 Provider 操作与旧模型移除 {#phase-9}

### 工作 {#phase-9-work}

1. same-provider rename/copy 使用 provider native fast path。
2. cross-provider copy 使用 streaming/range pipeline、version validation 和 progress。
3. cross-provider move 只有 copy 完成并验证后才 delete source；失败返回精确 partial
   outcome。
4. 删除旧 protobuf string path、旧 ProjectPath persistence 和专用 transfer RPC。
5. 删除不再使用的 `Fs` 方法、Local/Remote behavior branches 和 compatibility adapter。
6. 对旧 workspace/session 数据提供一次性 migration 或明确 invalidation policy。

### 退出条件 {#phase-9-exit}

- `VFS-EXP-013` 通过；
- 没有 caller 依赖旧 wire path；
- no-op adapter、dead RPC 和重复 cache 已删除；
- protocol downgrade 行为有明确错误或兼容策略。

## 15. 阶段 10：收敛与最终验证 {#phase-10}

1. 运行依赖、dead code、feature 和 compatibility caller audit。
2. 更新 architecture、remote development、Project Panel、ZIP 和 extension capability
   文档。
3. 将全部实验结果写入进度账本并保留复现命令。
4. 运行最终 gate：

```sh
cargo fmt --all -- --check
cargo test --workspace
./script/clippy
./script/check-philosophy
cargo check --locked -p docs_preprocessor
./script/generate-action-metadata
(cd docs && npx prettier --write src/)
(cd docs && npx prettier --check src/)
mdbook build docs
```

5. 对 macOS、Windows、WSL、Docker、SSH 和 network filesystem 执行 platform
   runbook。当前主机不能运行的项目标记 `NOT RUN`，不得写成 PASS。
6. 复核 `.rules` 候选，只在重复验证后在 PR 文本提出，不在实现中顺手修改。

### 退出条件 {#phase-10-exit}

- `VFS-EXP-014` 通过；
- 第 1 节完成定义逐条有证据；
- 进度账本没有未解释 gap；
- Goal 可以标记 complete。

## 16. 阶段验证矩阵 {#validation-matrix}

每阶段至少运行触及 crate 的 check/test/clippy：

```sh
cargo check --locked -p fs -p worktree -p project -p workspace
cargo test --locked -p fs -p worktree -p project -p workspace
./script/clippy -p fs -p worktree -p project -p workspace
```

触及 Editor、LSP、Git、媒体、remote 或 proto 时追加对应 package。Proto 变化必须运行
可用的 `buf lint` 和 `buf format --diff --exit-code`；如果本机没有 `buf`，进度账本
记录 `NOT RUN` 和 CI 替代验证。docs pre/postprocessor 只能由 mdBook 通过 JSON stdin
调用，不得把 `cargo run -p docs_preprocessor --` 或 `... -- postprocess` 当作独立测试。

每次验证记录完整命令和 exit status。不得只写“tests pass”。

## 17. 中断、阻塞与恢复 {#blocking-and-resume}

- 遇到技术困难时先缩小复现、查现有 tests、阅读 caller 和尝试替代方案。
- 需要用户选择且不同选择会改变产品语义、协议兼容或外部 authority 时才停下询问。
- 平台不可用不是整个 Goal blocked；写 runbook 并继续可执行阶段。
- 同一 blocker 达到 Goal blocked 条件时，更新进度账本，说明已尝试方案和所需输入。
- 新对话恢复时先读 goal、report、plan、experiments 和 progress，再从 progress 的
  `Next action` 继续。不得从 Phase 0 重做已验收工作。

## 18. 移交内容 {#handoff}

最终移交必须包含：

- 完成阶段和提交清单；
- provider/capability matrix；
- protocol version 与 compatibility policy；
- path encoding specification；
- experiment results；
- remaining platform `NOT RUN` runbooks；
- rejected designs 及原因；
- migration/deprecation removal 状态；
- release notes 与 `Suggested .rules additions` 候选。
