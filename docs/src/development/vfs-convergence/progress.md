---
title: VFS Convergence Progress
description: Durable execution ledger for the ZZZ VFS convergence and extension work.
---

# VFS 收敛与拓展进度账本

此文件是[VFS 收敛与拓展实现计划](./implementation-plan.md)的持久执行状态。每次开始
或恢复 Goal 时先读本文件，并从 `Next action` 继续。冻结决策见
[decisions.md](./decisions.md)，可复制命令见[goal.md](./goal.md)。

## 当前状态 {#current-status}

- Goal status: `NOT STARTED`
- Plan baseline: `main` @ `40536482fa`
- Current HEAD: `main` @ `40536482fa`（尚未创建实现分支）
- Branch/worktree: `vfs-convergence` @ `../ZZZ-vfs-convergence`（尚未创建）
- Active phase: `Phase 0`
- Last completed phase: `None`
- Blocking issue: `None`
- Next action: 执行阶段 0 的 bootstrap（创建 worktree、归档旧账本、记录 baseline、
  跑 `CONV-EXP-001`）。

## Bootstrap {#bootstrap}

阶段 0 开始前执行，结果记入下表。第 1 步不能省：`git worktree add ... main` 只检出已
提交内容，文档未提交时新 worktree 里没有计划，Goal 会立刻卡住。

```sh
# 1. 先在 main 上把这份文档集提交（代码基线仍是 40536482fa）
git add docs/src/SUMMARY.md docs/src/development/vfs-convergence
git commit -s -m "docs: add VFS convergence plan"
git log -1 --format=%H

# 2. 再建隔离 worktree
git worktree add ../ZZZ-vfs-convergence -b vfs-convergence main
cd ../ZZZ-vfs-convergence
git status --porcelain
git merge-base --is-ancestor 40536482fa HEAD && echo "code baseline 40536482fa included"
rustc -Vv
uname -a
```

| 项                     | 值  | 备注                             |
| ---------------------- | --- | -------------------------------- |
| 文档提交 SHA           |     | `docs: add VFS convergence plan` |
| Baseline HEAD          |     | 代码基线仍为 `40536482fa`        |
| `rustc -Vv`            |     |                                  |
| OS / kernel            |     |                                  |
| Filesystem / case 行为 |     |                                  |
| Remote transport       |     |                                  |
| 初始 `git status`      |     | 期望为空                         |

## 阶段状态 {#phase-status}

| Phase | Result                     | Status      | Commit | Validation | Notes |
| ----- | -------------------------- | ----------- | ------ | ---------- | ----- |
| 0     | 账本校准、worktree、基线   | NOT STARTED | -      | -          | -     |
| 1     | 可观测性与度量门禁         | NOT STARTED | -      | -          | -     |
| 2     | 核心退化修复               | NOT STARTED | -      | -          | -     |
| 3     | mount 生命周期             | NOT STARTED | -      | -          | -     |
| 4     | WatchSupervisor 与 delta   | NOT STARTED | -      | -          | -     |
| 5     | worktree 精确身份          | NOT STARTED | -      | -          | -     |
| 6     | capability 驱动与错误 i18n | NOT STARTED | -      | -          | -     |
| 7     | `Fs` 职责拆分              | NOT STARTED | -      | -          | -     |
| 8     | remote 客户端 snapshot     | NOT STARTED | -      | -          | -     |
| 9     | mount descriptor 持久化    | NOT STARTED | -      | -          | -     |
| 10    | `VfsUri` 与 codec          | NOT STARTED | -      | -          | -     |
| 11    | 扩展只读 FS API            | NOT STARTED | -      | -          | -     |
| 12    | 写入语义补全               | NOT STARTED | -      | -          | -     |
| 13    | remote 到 remote 直传      | NOT STARTED | -      | -          | -     |
| 14    | 新 provider                | NOT STARTED | -      | -          | -     |
| 15    | 增量索引                   | NOT STARTED | -      | -          | -     |
| 16    | 收敛与全量验证             | NOT STARTED | -      | -          | -     |

## 实验账本 {#experiments}

状态取值：`PASS`、`FAIL`、`REJECTED`、`BLOCKED`、`NOT RUN`。`NOT RUN` 必须附 runbook
路径与原因。

| ID           | 阶段 | Status  | Command / Evidence | Result | Notes |
| ------------ | ---- | ------- | ------------------ | ------ | ----- |
| CONV-EXP-001 | 0    | PENDING | -                  | -      | -     |
| CONV-EXP-002 | 1    | PENDING | -                  | -      | -     |
| CONV-EXP-003 | 2    | PENDING | -                  | -      | -     |
| CONV-EXP-004 | 2    | PENDING | -                  | -      | -     |
| CONV-EXP-005 | 2    | PENDING | -                  | -      | -     |
| CONV-EXP-006 | 4    | PENDING | -                  | -      | -     |
| CONV-EXP-007 | 5    | PENDING | -                  | -      | -     |
| CONV-EXP-008 | 3    | PENDING | -                  | -      | -     |
| CONV-EXP-009 | 5    | PENDING | -                  | -      | -     |
| CONV-EXP-010 | 6    | PENDING | -                  | -      | -     |
| CONV-EXP-011 | 6    | PENDING | -                  | -      | -     |
| CONV-EXP-012 | 7    | PENDING | -                  | -      | -     |
| CONV-EXP-013 | 8    | PENDING | -                  | -      | -     |
| CONV-EXP-014 | 9    | PENDING | -                  | -      | -     |
| CONV-EXP-015 | 10   | PENDING | -                  | -      | -     |
| CONV-EXP-016 | 11   | PENDING | -                  | -      | -     |
| CONV-EXP-017 | 13   | PENDING | -                  | -      | -     |
| CONV-EXP-018 | 12   | PENDING | -                  | -      | -     |
| CONV-EXP-019 | 14   | PENDING | -                  | -      | -     |
| CONV-EXP-020 | 15   | PENDING | -                  | -      | -     |

## 度量基线 {#metrics}

阶段 1 建立，之后每阶段更新。数值是单调性检查，不是绝对目标。

前两条是必须归零的硬门禁，后三条是单调性检查。判定规则与
[实现计划第 24 节](./implementation-plan.md#metrics)一致。

| 指标                                    | 基线 | 当前 |
| --------------------------------------- | ---- | ---- |
| `vfs` 之外 `MountId::new` 出现数        | 待测 | -    |
| 扫描层 UTF-8 往返与 `.log_err()` 丢弃数 | 待测 | -    |
| 手写 capability/错误判断数              | 待测 | -    |
| `crates/vfs` tracing 调用数             | 0    | -    |
| `trait Fs` 的 service supertrait 数     | 3    | -    |

对应命令：

```sh
rg -n 'MountId::new' crates --glob '*.rs'
rg -c -e 'to_str\(\)' -e 'provider_path_to_legacy_utf8' -e '\.log_err\(\)' \
  crates/worktree/src/worktree.rs
rg -c -e 'supports_trash_restore' -e 'contains\("File is too large' crates --glob '*.rs'
rg -c 'tracing::' crates/vfs/src/*.rs
rg -n 'pub trait Fs' crates/fs/src/fs.rs
```

## 已知限制 {#known-limits}

计划接受、但必须在收尾时向用户明示的限制。新增条目需同时更新
[decisions.md](./decisions.md#open-risks)。

| 限制                                                 | 引入阶段 | 影响                                     | 缓解                                                         |
| ---------------------------------------------------- | -------- | ---------------------------------------- | ------------------------------------------------------------ |
| remote `restore_token` 只在当前会话有效              | 阶段 6   | 重启后无法 restore 之前 trash 的远端条目 | `ResourceActionAvailability.restore` 在无 token 时为 `false` |
| 显示路径冲突（两个 byte name 映射到同一 lossy 路径） | 阶段 5   | 面板上出现同名条目                       | 操作以 `resource_id` 为准；记录冲突计数并 warn               |

## 兼容层清单 {#compatibility}

每个条目必须有调用点清单、删除阶段与测试。移除前在账本记录“调用点为 0”的证据。

| Adapter                                  | 引入阶段 | 状态   | 移除阶段 | Notes                              |
| ---------------------------------------- | -------- | ------ | -------- | ---------------------------------- |
| `UpdateWorktree` 快照推送                | 阶段 4   | ACTIVE | 阶段 16  | 阶段 4 接 watch 后保留一个发布周期 |
| `ProviderPathV2` 旧持久化行              | 阶段 9   | ACTIVE | 阶段 9   | 读取时推断 mount key 并立即重写    |
| `supports_trash_restore` 旧实现          | 阶段 6   | ACTIVE | 阶段 6   | 改为 capability 派生               |
| `File.is_local = vfs_snapshot.is_some()` | 阶段 6   | ACTIVE | 阶段 6   | 由 capability 派生                 |
| `terminal_mode` 手工 mount id 偏移       | 阶段 3   | ACTIVE | 阶段 3   | 删除 magic offset                  |

## 旧账本归档 {#archive}

`docs/src/development/vfs-refactor-progress.md` 在阶段 0 归档。

| 项                         | 值  |
| -------------------------- | --- |
| 归档时 HEAD                |     |
| 最后一阶段结论             |     |
| Phase 9 真实提交数         |     |
| 与新账本的衔接说明写入日期 |     |

## 阶段记录模板 {#template}

```markdown
### Phase N：<名称>

- Baseline: `<SHA>`
- 决策: 引用的 decisions.md 条目，或新增决策及理由
- 改动: 每个提交一行，`<SHA>` 加一句结构或行为变化
- 实验: `<CONV-EXP-NNN>` PASS/FAIL，命令与摘要
- 验证: 命令 + exit status
- 剩余风险:
- Next action:
```
