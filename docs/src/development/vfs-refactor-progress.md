---
title: VFS Refactoring Progress
description: Durable execution ledger for the complete ZZZ VFS refactor.
---

# VFS 重构进度账本

此文件是
[VFS 完整重构执行计划](./vfs-research/refactoring-plan.md)的持久执行状态。每次
开始或恢复 Goal 时先读取本文件，并从 `Next action` 继续。

## 当前状态 {#current-status}

- Goal status: `NOT STARTED`
- Baseline HEAD: `6500fbdeccd7d523acfc69161d6371b631fdb6ac`
- Current HEAD: `NOT RECORDED`
- Branch/worktree: `NOT RECORDED`
- Active phase: `Phase 0`
- Last completed phase: `None`
- Blocking issue: `None`
- Next action: 记录当前工作树、HEAD、平台、toolchain 和 Phase 0 baseline。

## 阶段状态 {#phase-status}

| Phase | Result                             | Status      | Commit | Validation | Notes |
| ----- | ---------------------------------- | ----------- | ------ | ---------- | ----- |
| 0     | Baseline、ADR、实验 harness        | NOT STARTED | -      | -          | -     |
| 1     | Path/resource types 与 v2 wire     | NOT STARTED | -      | -          | -     |
| 2     | Provider、LocalProvider、职责拆分  | NOT STARTED | -      | -          | -     |
| 3     | Snapshot、ResourceId、Worktree     | NOT STARTED | -      | -          | -     |
| 4     | RemoteProviderProxy 与 VFS RPC     | NOT STARTED | -      | -          | -     |
| 5     | Consumer 迁移                      | NOT STARTED | -      | -          | -     |
| 6     | LSP/Git/native execution           | NOT STARTED | -      | -          | -     |
| 7     | ArchiveProvider 与 ZIP             | NOT STARTED | -      | -          | -     |
| 8     | Composition layers 与 overlay 决策 | NOT STARTED | -      | -          | -     |
| 9     | Cross-provider 与旧模型移除        | NOT STARTED | -      | -          | -     |
| 10    | 收敛与最终验证                     | NOT STARTED | -      | -          | -     |

## Baseline {#baseline}

执行 Phase 0 时填写：

- Git status:
- Rust toolchain:
- Host OS/architecture:
- Filesystem and case behavior:
- Remote targets:
- Workspace size and fixture revisions:
- Existing failures:
- Raw output directory:

## 设计决策 {#decisions}

| Decision                        | Status | ADR/path | Rationale | Consequence |
| ------------------------------- | ------ | -------- | --------- | ----------- |
| Path encoding                   | OPEN   | -        | -         | -           |
| ResourceId lifetime             | OPEN   | -        | -         | -           |
| Provider capability model       | OPEN   | -        | -         | -           |
| Positioned I/O                  | OPEN   | -        | -         | -           |
| Snapshot freshness and eviction | OPEN   | -        | -         | -           |
| Watch sequence and overflow     | OPEN   | -        | -         | -           |
| Remote retry and idempotency    | OPEN   | -        | -         | -           |
| Symlink and containment         | OPEN   | -        | -         | -           |
| Atomicity and expected version  | OPEN   | -        | -         | -           |
| LSP/native mapping              | OPEN   | -        | -         | -           |
| Archive limits                  | OPEN   | -        | -         | -           |

## Compatibility adapter 清单 {#compatibility-adapters}

| Adapter | Introduced | Callers | Removal phase | Status |
| ------- | ---------- | ------- | ------------- | ------ |
| None    | -          | -       | -             | -      |

## 实验结果 {#experiments}

结果定义在
[experiments.tsv](./vfs-research/experiments.tsv)。不要删除失败或被拒绝的实验。
实验总状态只记录必做验收结果；`VFS-EXP-012` 的可选 overlay 结论单独写入
`Decision`，即使 overlay 为 `REJECTED`，必做 composition 验收仍必须为 `PASS`。

| Experiment  | Status  | Command/data | Result | Decision |
| ----------- | ------- | ------------ | ------ | -------- |
| VFS-EXP-001 | NOT RUN | -            | -      | -        |
| VFS-EXP-002 | NOT RUN | -            | -      | -        |
| VFS-EXP-003 | NOT RUN | -            | -      | -        |
| VFS-EXP-004 | NOT RUN | -            | -      | -        |
| VFS-EXP-005 | NOT RUN | -            | -      | -        |
| VFS-EXP-006 | NOT RUN | -            | -      | -        |
| VFS-EXP-007 | NOT RUN | -            | -      | -        |
| VFS-EXP-008 | NOT RUN | -            | -      | -        |
| VFS-EXP-009 | NOT RUN | -            | -      | -        |
| VFS-EXP-010 | NOT RUN | -            | -      | -        |
| VFS-EXP-011 | NOT RUN | -            | -      | -        |
| VFS-EXP-012 | NOT RUN | -            | -      | -        |
| VFS-EXP-013 | NOT RUN | -            | -      | -        |
| VFS-EXP-014 | NOT RUN | -            | -      | -        |

## 验证记录 {#validation-log}

| Date | Phase | Command | Exit | Result | Existing failure? |
| ---- | ----- | ------- | ---- | ------ | ----------------- |
| -    | -     | -       | -    | -      | -                 |

## 提交记录 {#commit-log}

| Commit | Phase | Summary | Validation | Reversible boundary |
| ------ | ----- | ------- | ---------- | ------------------- |
| -      | -     | -       | -          | -                   |

## 平台 QA {#platform-qa}

| Platform           | Status  | Runbook/result | Remaining risk |
| ------------------ | ------- | -------------- | -------------- |
| Linux              | NOT RUN | -              | -              |
| macOS              | NOT RUN | -              | -              |
| Windows            | NOT RUN | -              | -              |
| WSL                | NOT RUN | -              | -              |
| Docker remote      | NOT RUN | -              | -              |
| SSH remote         | NOT RUN | -              | -              |
| Network filesystem | NOT RUN | -              | -              |

## Remaining work {#remaining-work}

- Phase 0 through Phase 10.

## Next action {#next-action}

记录当前工作树和执行 baseline，然后开始 Phase 0 的现有行为验证、ADR 与 provider
conformance harness。
