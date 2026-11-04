---
title: Database Explorer Progress
description: Durable execution ledger for the ZZZ Database Explorer implementation.
---

# 数据库浏览器执行账本

此文件是[实现计划](./implementation-plan.md)的持久执行状态。每次开始或恢复执行时先读
本文件，并从 `Next action` 继续。冻结决策见 [decisions.md](./decisions.md)，可复制提示词
见 [goal.md](./goal.md)。

## 当前状态 {#current-status}

- Goal status: `NOT STARTED`
- Plan baseline: `main` @ `3af1429a83`
- Current HEAD: `main` @ `3af1429a83`（尚未创建实现分支）
- Branch/worktree: 尚未创建
- Active phase: `Phase 0`
- Last completed phase: `None`
- Blocking issue: `None`
- Next action: 执行 Phase 0 的 bootstrap（见下方）。

## Bootstrap {#bootstrap}

Phase 0 开始前执行，结果记入下表。第 1 步不能省：worktree 只检出已提交内容，
文档未提交时新工作树里没有计划，执行会立刻卡住。

```sh
# 1. 先在 main 上把这份文档集提交（代码基线仍是 3af1429a83）
git add docs/src/SUMMARY.md docs/src/development/database
git commit -s -m "docs: add database explorer plan"
git log -1 --format=%H

# 2. 再建隔离 worktree
git worktree add ../ZZZ-database-explorer -b database-explorer main
cd ../ZZZ-database-explorer
git status --porcelain
git merge-base --is-ancestor 3af1429a83 HEAD && echo "code baseline 3af1429a83 included"
rustc -Vv
uname -a
```

| 项                | 值  | 备注                               |
| ----------------- | --- | ---------------------------------- |
| 文档提交 SHA      |     | `docs: add database explorer plan` |
| Baseline HEAD     |     | 代码基线仍为 `3af1429a83`          |
| `rustc -Vv`       |     |                                    |
| OS / kernel       |     |                                    |
| 初始 `git status` |     | 期望为空                           |

### Clean baseline 验证 {#baseline-validation}

| 命令                                   | Exit status | 既有失败登记 |
| -------------------------------------- | ----------- | ------------ |
| `cargo xtask check`                    |             |              |
| `cargo test -p csv_preview`            |             |              |
| `cd docs && npx prettier --check src/` |             |              |
| `mdbook build docs`                    |             |              |

## 阶段状态 {#phase-status}

| Phase | Result                   | Status      | Commit | Validation | Notes |
| ----- | ------------------------ | ----------- | ------ | ---------- | ----- |
| 0     | 账本校准、worktree、基线 | NOT STARTED | -      | -          | -     |
| 1     | 骨架与 SQLite 只读浏览   | NOT STARTED | -      | -          | -     |
| 2     | 连接管理与凭据           | NOT STARTED | -      | -          | -     |
| 3     | 查询执行与取消           | NOT STARTED | -      | -          | -     |
| 4     | 结果集视图               | NOT STARTED | -      | -          | -     |
| 5     | 编辑、事务与确认         | NOT STARTED | -      | -          | -     |
| 6     | 扩展驱动协议             | NOT STARTED | -      | -          | -     |
| 7     | ER 图与导入导出          | NOT STARTED | -      | -          | -     |
| 8     | Visual EXPLAIN           | NOT STARTED | -      | -          | -     |
| 9     | 查询历史                 | NOT STARTED | -      | -          | -     |
| 10    | Notebook 式 SQL          | NOT STARTED | -      | -          | -     |
| 11    | 远程与 SSH（条件项）     | NOT STARTED | -      | -          | -     |
| 12    | MCP 只读暴露             | NOT STARTED | -      | -          | -     |
| 13    | 文档、门禁与收尾         | NOT STARTED | -      | -          | -     |

## 现成件复核 {#reuse-review}

Phase 0 第 4 步填写：确认下列实际签名，并据此定稿计划第 5.2 节契约草案。

| 现成件                       | 实际签名 / 位置                    | 复核结论 |
| ---------------------------- | ---------------------------------- | -------- |
| `workspace::Item`            | `crates/workspace/src/item.rs:189` |          |
| `workspace::Panel`           | `crates/workspace/src/dock.rs:40`  |          |
| `async_trait` 用法           | `crates/fs/src/fs.rs:102`          |          |
| `CredentialsProvider`        | `crates/credentials_provider/...`  |          |
| `DatabaseZap` 图标           | `crates/icons/src/icons.rs:87`     |          |
| `mermaid_render` `erDiagram` | `crates/mermaid_render/...:219`    |          |
| `csv_preview::init` 注册     | `crates/zzz/src/main.rs:687`       |          |

## 实验记录 {#experiments}

格式：`DB-EXP-NNN`、目的、命令、baseline 结果、改动后结果、结论（PASS/FAIL/REJECTED）。

| ID         | 目的                                           | 命令 | Baseline | After | 结论 | Phase |
| ---------- | ---------------------------------------------- | ---- | -------- | ----- | ---- | ----- |
| DB-EXP-001 | SQLite 连接并发与 `sqlite3_interrupt` 线程模型 | 待定 |          |       |      | 1     |
| DB-EXP-002 | 大结果集窗口化拉取的内存上限                   | 待定 |          |       |      | 4     |
| DB-EXP-003 | 扩展子进程协议跨平台换行/编码/超时             | 待定 |          |       |      | 6     |

## 决策变更记录 {#decision-changes}

执行期若需变更 [decisions.md](./decisions.md) 的冻结决策，先在此登记理由与影响，
再修改 decisions.md，最后改实现。

| 决策 | 原结论 | 新结论 | 理由 | 影响阶段 | 日期 |
| ---- | ------ | ------ | ---- | -------- | ---- |
|      |        |        |      |          |      |

## 提交记录 {#commits}

| SHA | 提交信息 | Phase | 验证 | 备注 |
| --- | -------- | ----- | ---- | ---- |
|     |          |       |      |      |

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
