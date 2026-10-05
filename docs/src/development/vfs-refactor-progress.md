---
title: VFS Refactoring Progress
description: Durable execution ledger for the complete ZZZ VFS refactor.
---

# VFS 重构进度账本

此文件是
[VFS 完整重构执行计划](./vfs-research/refactoring-plan.md)的持久执行状态。每次
开始或恢复 Goal 时先读取本文件，并从 `Next action` 继续。

## 当前状态 {#current-status}

- Goal status: `ACTIVE`
- Baseline HEAD: `6500fbdeccd7d523acfc69161d6371b631fdb6ac`
- Current HEAD: `5f4066d786b17b17ec1d3e222516e061380204bc` plus uncommitted
  Phase 2A changes
- Branch/worktree: `vfs-refactor` in the primary worktree
- Active phase: `Phase 2`
- Last completed phase: `Phase 1`
- Blocking issue: `None`
- Next action: 提交 Phase 2A provider contract/MemoryProvider，然后实现
  LegacyFsProvider、LocalProvider、native positioned I/O 与 watch adapter。

## 阶段状态 {#phase-status}

| Phase | Result                             | Status      | Commit       | Validation | Notes                               |
| ----- | ---------------------------------- | ----------- | ------------ | ---------- | ----------------------------------- |
| 0     | Baseline、ADR、实验 harness        | PASS        | `4c35091470` | PASS       | VFS-EXP-001 PASS                    |
| 1     | Path/resource types 与 v2 wire     | PASS        | `86b96e7a75` | PASS       | VFS-EXP-002 PASS                    |
| 2     | Provider、LocalProvider、职责拆分  | IN PROGRESS | -            | PARTIAL    | 2A contract and MemoryProvider pass |
| 3     | Snapshot、ResourceId、Worktree     | NOT STARTED | -            | -          | -                                   |
| 4     | RemoteProviderProxy 与 VFS RPC     | NOT STARTED | -            | -          | -                                   |
| 5     | Consumer 迁移                      | NOT STARTED | -            | -          | -                                   |
| 6     | LSP/Git/native execution           | NOT STARTED | -            | -          | -                                   |
| 7     | ArchiveProvider 与 ZIP             | NOT STARTED | -            | -          | -                                   |
| 8     | Composition layers 与 overlay 决策 | NOT STARTED | -            | -          | -                                   |
| 9     | Cross-provider 与旧模型移除        | NOT STARTED | -            | -          | -                                   |
| 10    | 收敛与最终验证                     | NOT STARTED | -            | -          | -                                   |

## Baseline {#baseline}

- Git status: initial `## main...origin/main`, no modified or untracked files; implementation
  branch created before edits.
- Rust toolchain: `rustc 1.99.0 (b940084d7 2026-09-28)`; Cargo
  `1.99.0 (5f94df478 2026-08-27)`; host `x86_64-unknown-linux-gnu`.
- Host OS/architecture: Fedora Linux 46 prerelease, kernel
  `7.3.0-0.rc4.260925g165768bb7026.42.fc46.x86_64`, x86_64.
- Filesystem and case behavior: Btrfs, 4,096-byte block, case-sensitive probe at
  `.tmp/vfs-refactor/case-probe.JObzw6`.
- Remote targets: installed Rust targets include Linux GNU/musl, Windows GNU, macOS x86_64,
  wasm32 and WASI. `podman` and `ssh` are available; Docker daemon, WSL, macOS, Windows,
  network filesystem and an external SSH authority were not used in Phase 0.
- Workspace size and fixture revisions: 5,220 tracked files and 141,549,315 tracked bytes;
  source tree excluding `.git`, `target` and `.tmp` is 440 MiB. `Cargo.lock` SHA-256 is
  `9feb95fb75381758ae2cc4f09932e29cfd776d6111f4c6dbc956e95d12eecadb` at baseline.
  Cross-platform path corpus schema is `1`, seed `1592614637`.
- Existing architecture counters from fixed `rg` commands: 300 `Arc<dyn Fs>`/`&dyn Fs`
  occurrences, 571 legacy `fs.<operation>` call sites, 49 `Worktree::Local/Remote` matches,
  64 `.is_local()` calls, 298 `RelPath` proto conversion hits and 143 `to_string_lossy()`
  hits in the scoped crates. These are broad migration counters, not all defects.
- Scan benchmark: release runtime on the current repository reports 5,221 files, 1,158
  directories, 72.26 ms scan time and 18,292 KiB maximum process RSS.
- Existing failures: reproduced and classified in the next section.
- Raw output directory: `.tmp/vfs-refactor/` (ignored, local only).

### Phase 0 existing failures {#phase-0-existing-failures}

All entries reproduce on the clean implementation baseline before VFS code changes.

| Crate    | Test                                                                                 | Reproduction        | Classification                                                    |
| -------- | ------------------------------------------------------------------------------------ | ------------------- | ----------------------------------------------------------------- |
| worktree | `test_load_file_encoding`                                                            | exit 101            | ISO-2022-JP fixture remains undecoded at `worktree_tests.rs:5881` |
| project  | `context_server_store::test_multi_worktree_context_server_settings`                  | exit 101            | project settings discovery, unrelated to VFS implementation       |
| project  | `context_server_store::test_multi_worktree_duplicate_context_server_first_wins`      | exit 101            | project settings discovery, unrelated to VFS implementation       |
| project  | `search::test_multiline_regex_crlf`                                                  | exit 101            | existing CRLF multiline search mismatch at `search.rs:155`        |
| project  | `lsp_store::test_other_adapters_lsp_configuration_contributions_are_unioned`         | exit 101            | existing LSP settings merge mismatch at `lsp_store.rs:774`        |
| project  | `lsp_store::test_user_initialization_options_override_adapter_arrays`                | exit 101            | existing LSP settings merge mismatch at `lsp_store.rs:650`        |
| project  | `test_staging_hunks`                                                                 | exit 101            | existing Git hunk range mismatch at `project_tests.rs:11274`      |
| project  | `test_git_events_after_project_excludes_dot_git`                                     | exit 101            | libgit2 rejects `other-branch` at `project_tests.rs:12345`        |
| editor   | `editor_tests::test_auto_formatter_skips_server_without_formatting`                  | exit 101            | existing formatter result mismatch at `editor_tests.rs:16796`     |
| editor   | `editor_tests::test_document_format_during_save`                                     | exit 101            | existing formatter result mismatch at `editor_tests.rs:16622`     |
| editor   | `editor_tests::test_format_echoing_received_line_endings_keeps_cursor`               | exit 101            | existing formatter whitespace mismatch                            |
| editor   | `editor_tests::test_join_lines_rust_block_comments`                                  | exit 101            | existing join-lines mismatch at `editor_tests.rs:7657`            |
| editor   | `editor_tests::test_multibuffer_format_during_save`                                  | exit 101            | existing multibuffer formatter mismatch                           |
| editor   | `inlays::inlay_hints::tests::test_no_hint_duplication_when_refresh_races_with_fetch` | exit 101            | existing race reaches `unwrap` at `inlay_hints.rs:1626`           |
| editor   | `editor_tests::test_race_in_multibuffer_save`                                        | exit 124 after 90 s | deterministic timeout when run alone                              |
| editor   | `editor_tests::test_range_format_on_save_success`                                    | exit 124 after 90 s | deterministic timeout when run alone                              |
| editor   | `editor_tests::test_range_format_respects_language_tab_size_override`                | exit 124 after 90 s | deterministic timeout when run alone                              |

## 设计决策 {#decisions}

| Decision                        | Status   | ADR/path                                                       | Rationale                        | Consequence                                |
| ------------------------------- | -------- | -------------------------------------------------------------- | -------------------------------- | ------------------------------------------ |
| Path encoding                   | ACCEPTED | [ADR](./vfs-research/adr.md#exact-path-wire-format)            | Lossless component bytes         | Display is never identity                  |
| ResourceId lifetime             | ACCEPTED | [ADR](./vfs-research/adr.md#resource-identity-lifetime)        | Session-stable identity          | Persistence re-interns mount/path          |
| Provider capability model       | ACCEPTED | [ADR](./vfs-research/adr.md#provider-capability-model)         | Structured limits and guarantees | Wrappers recompute capabilities            |
| Positioned I/O                  | ACCEPTED | [ADR](./vfs-research/adr.md#positioned-io-and-cancellation)    | Avoid shared cursor races        | Cursor remains an adapter                  |
| Snapshot freshness and eviction | ACCEPTED | [ADR](./vfs-research/adr.md#snapshot-freshness-and-budgets)    | Bounded lazy state               | Fixed Phase 3 budgets                      |
| Watch sequence and overflow     | ACCEPTED | [ADR](./vfs-research/adr.md#watch-sequence-and-reconciliation) | Events are hints                 | Gaps force scoped rescan                   |
| Remote retry and idempotency    | ACCEPTED | [ADR](./vfs-research/adr.md#remote-retry-and-idempotency)      | No blind mutation replay         | Result journal required                    |
| Symlink and containment         | ACCEPTED | [ADR](./vfs-research/adr.md#symlink-and-containment)           | Host validates authority         | Follow is explicit                         |
| Atomicity and expected version  | ACCEPTED | [ADR](./vfs-research/adr.md#atomicity-and-expected-version)    | Never invent guarantees          | Stale versions are typed errors            |
| LSP/native mapping              | ACCEPTED | [ADR](./vfs-research/adr.md#lsp-and-native-mapping)            | Native paths stay data-local     | Virtual resources do not impersonate files |
| Archive limits                  | ACCEPTED | [ADR](./vfs-research/adr.md#archive-limits)                    | Bound hostile archives           | Provider stays read-only                   |

## Compatibility adapter 清单 {#compatibility-adapters}

| Adapter                                    | Introduced | Callers                                                                              | Removal phase | Status    |
| ------------------------------------------ | ---------- | ------------------------------------------------------------------------------------ | ------------- | --------- |
| Legacy `Fs` storage façade                 | Existing   | 571 broad operation call sites                                                       | Phase 9       | BASELINE  |
| Generic UTF-8 `ProviderPath` bridge        | Phase 1    | `ProjectPath` dual-read/write; [inventory](./vfs-research/path-adapter-inventory.md) | Phase 9       | ACTIVE    |
| `WorktreeId` to temporary `MountId`        | Phase 1    | `ProjectPath` v2 adapter                                                             | Phase 3       | ACTIVE    |
| `RelPath`/`ProjectPath` UTF-8 wire         | Existing   | Caller groups in [inventory](./vfs-research/path-adapter-inventory.md)               | Phase 9       | MIGRATING |
| `Worktree::Local/Remote` behavior branches | Existing   | 49 enum matches and 64 `is_local` calls                                              | Phase 5/9     | BASELINE  |
| Dedicated image/download byte RPC          | Existing   | Project media/download handlers                                                      | Phase 5/9     | BASELINE  |

## 实验结果 {#experiments}

结果定义在
[experiments.tsv](./vfs-research/experiments.tsv)。不要删除失败或被拒绝的实验。
实验总状态只记录必做验收结果；`VFS-EXP-012` 的可选 overlay 结论单独写入
`Decision`，即使 overlay 为 `REJECTED`，必做 composition 验收仍必须为 `PASS`。

| Experiment  | Status  | Command/data                                                  | Result                                                  | Decision                                          |
| ----------- | ------- | ------------------------------------------------------------- | ------------------------------------------------------- | ------------------------------------------------- |
| VFS-EXP-001 | PASS    | Commands in validation log and `.tmp/vfs-refactor/`           | Existing failures reproduced; scan and RSS recorded     | Baseline frozen at `a3a0f97340`                   |
| VFS-EXP-002 | PASS    | `cargo test --locked -p vfs -p proto` and cross-target checks | 10 valid fixtures round-trip; 6 invalid fixtures reject | Adopt component bytes plus explicit encoding/root |
| VFS-EXP-003 | NOT RUN | -                                                             | -                                                       | -                                                 |
| VFS-EXP-004 | NOT RUN | -                                                             | -                                                       | -                                                 |
| VFS-EXP-005 | NOT RUN | -                                                             | -                                                       | -                                                 |
| VFS-EXP-006 | NOT RUN | -                                                             | -                                                       | -                                                 |
| VFS-EXP-007 | NOT RUN | -                                                             | -                                                       | -                                                 |
| VFS-EXP-008 | NOT RUN | -                                                             | -                                                       | -                                                 |
| VFS-EXP-009 | NOT RUN | -                                                             | -                                                       | -                                                 |
| VFS-EXP-010 | NOT RUN | -                                                             | -                                                       | -                                                 |
| VFS-EXP-011 | NOT RUN | -                                                             | -                                                       | -                                                 |
| VFS-EXP-012 | NOT RUN | -                                                             | -                                                       | -                                                 |
| VFS-EXP-013 | NOT RUN | -                                                             | -                                                       | -                                                 |
| VFS-EXP-014 | NOT RUN | -                                                             | -                                                       | -                                                 |

## 验证记录 {#validation-log}

| Date       | Phase | Command                                                                                                                    | Exit     | Result                                                                                             | Existing failure?                                                       |
| ---------- | ----- | -------------------------------------------------------------------------------------------------------------------------- | -------- | -------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| 2026-10-05 | 0     | `cargo check --locked -p fs -p worktree -p project -p workspace -p editor`                                                 | 0        | PASS                                                                                               | No                                                                      |
| 2026-10-05 | 0     | `cargo test --locked -p fs -p worktree -p project`                                                                         | 101      | FAIL                                                                                               | Yes: 7 project failures; fs 23 passed, 1 ignored                        |
| 2026-10-05 | 0     | `cargo test --locked -p worktree`                                                                                          | 101      | FAIL                                                                                               | Yes: 88 passed, `test_load_file_encoding` failed                        |
| 2026-10-05 | 0     | `cargo test --locked -p workspace -p editor`                                                                               | 1        | FAIL/TIMEOUT                                                                                       | Yes: interrupted after six failures and three tests stalled beyond 60 s |
| 2026-10-05 | 0     | `cargo test --locked -p workspace`                                                                                         | 0        | PASS: 235 tests                                                                                    | No                                                                      |
| 2026-10-05 | 0     | `cargo test --locked -p remote`                                                                                            | 0        | PASS: 34 tests                                                                                     | No                                                                      |
| 2026-10-05 | 0     | `cargo test --locked -p rpc`                                                                                               | 0        | PASS: 6 tests                                                                                      | No                                                                      |
| 2026-10-05 | 0     | `cargo test --locked -p image_viewer -p pdf_viewer -p audio_viewer -p video_viewer -p typst_preview`                       | 0        | PASS: 54 tests                                                                                     | No                                                                      |
| 2026-10-05 | 0     | `./script/clippy -p fs -p worktree -p project -p workspace -p editor`                                                      | 0        | PASS                                                                                               | No                                                                      |
| 2026-10-05 | 0     | seven individual `cargo test --locked -p project --test integration <name> -- --exact --nocapture` runs                    | 101 each | FAIL reproduced                                                                                    | Yes                                                                     |
| 2026-10-05 | 0     | six individual `cargo test --locked -p editor <name> -- --exact --nocapture --test-threads=1` runs                         | 101 each | FAIL reproduced                                                                                    | Yes                                                                     |
| 2026-10-05 | 0     | three individual `timeout 90s cargo test --locked -p editor <name> -- --exact --nocapture --test-threads=1` runs           | 124 each | TIMEOUT reproduced                                                                                 | Yes                                                                     |
| 2026-10-05 | 0     | `target/release/worktree_benchmarks .` under `/usr/bin/time -v`                                                            | 0        | PASS: 72.26 ms, 18,292 KiB RSS                                                                     | No                                                                      |
| 2026-10-05 | 0     | `cargo test --locked -p vfs`                                                                                               | 0        | PASS: 3 tests                                                                                      | No                                                                      |
| 2026-10-05 | 0     | FakeFs and RealFs legacy conformance filters                                                                               | 0 each   | PASS                                                                                               | No                                                                      |
| 2026-10-05 | 0     | `cargo fmt --all -- --check`                                                                                               | 0        | PASS                                                                                               | No                                                                      |
| 2026-10-05 | 0     | `cargo check --locked -p vfs -p fs`                                                                                        | 0        | PASS                                                                                               | No                                                                      |
| 2026-10-05 | 0     | `cargo test --locked -p fs`                                                                                                | 0        | PASS: 25 passed, 1 ignored                                                                         | No                                                                      |
| 2026-10-05 | 0     | `./script/clippy -p vfs -p fs`                                                                                             | 0        | PASS                                                                                               | No                                                                      |
| 2026-10-05 | 0     | `(cd docs && npx prettier --check src/)`                                                                                   | 0        | PASS                                                                                               | No                                                                      |
| 2026-10-05 | 0     | touched-diff P0 detectors and manual P0/P1/P2 review                                                                       | 0        | PASS: frame-delay semantics fixed; no remaining finding                                            | No                                                                      |
| 2026-10-05 | 1     | `cargo fmt --all -- --check`                                                                                               | 0        | PASS                                                                                               | No                                                                      |
| 2026-10-05 | 1     | `cargo check --locked -p vfs -p proto -p project`                                                                          | 0        | PASS                                                                                               | No                                                                      |
| 2026-10-05 | 1     | `cargo test --locked -p vfs -p proto`                                                                                      | 0        | PASS: 17 tests                                                                                     | No                                                                      |
| 2026-10-05 | 1     | `cargo test --locked -p project --test integration test_project_path_v2_wire_round_trip_and_mismatch_rejection -- --exact` | 0        | PASS: 1 test                                                                                       | No                                                                      |
| 2026-10-05 | 1     | `cargo check --locked -p vfs --target x86_64-pc-windows-gnu`                                                               | 0        | PASS                                                                                               | No                                                                      |
| 2026-10-05 | 1     | `cargo check --locked -p vfs --target x86_64-apple-darwin`                                                                 | 0        | PASS                                                                                               | No                                                                      |
| 2026-10-05 | 1     | `cargo test --locked -p project`                                                                                           | 101      | FAIL: 286 passed, 7 failed, 3 ignored                                                              | Yes: exact Phase 0 project failure set; new v2 test passed              |
| 2026-10-05 | 1     | `./script/clippy -p vfs -p proto -p project`                                                                               | 0        | PASS                                                                                               | No                                                                      |
| 2026-10-05 | 1     | `buf lint crates/proto/proto` and `buf format --diff --exit-code crates/proto/proto`                                       | -        | NOT RUN: `buf` unavailable                                                                         | Tool unavailable; prost build and proto tests PASS                      |
| 2026-10-05 | 1     | `(cd docs && npx prettier --check src/)`                                                                                   | 0        | PASS                                                                                               | No                                                                      |
| 2026-10-05 | 1     | touched-diff P0 detectors and manual P0/P1/P2 review                                                                       | 0        | PASS: fixed drive-case loss, boolean state model and intermediate collection; no remaining finding | No                                                                      |
| 2026-10-06 | 2A    | `cargo test --locked -p vfs`                                                                                               | 0        | PASS: 11 tests including shared provider conformance and watch overflow                            | No                                                                      |
| 2026-10-06 | 2A    | `./script/clippy -p vfs`                                                                                                   | 0        | PASS                                                                                               | No                                                                      |
| 2026-10-06 | 2A    | `cargo check --locked -p vfs --target x86_64-pc-windows-gnu` and `x86_64-apple-darwin`                                     | 0 each   | PASS                                                                                               | No                                                                      |
| 2026-10-06 | 2A    | touched-diff P0 detectors and manual P0/P1/P2 review                                                                       | 0        | PASS: fixed bounded paging and ID/version/sequence exhaustion semantics; no remaining finding      | No                                                                      |

## 提交记录 {#commit-log}

| Commit                                     | Phase | Summary                                                                               | Validation                                                                                             | Reversible boundary                                                                             |
| ------------------------------------------ | ----- | ------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------- |
| `4c350914704df52cf4733b57a074c91db25afab2` | 0     | Baseline, ADR, fixed corpus, fault transport and legacy conformance                   | Targeted fmt/check/test/clippy and docs Prettier PASS; existing failures recorded                      | Remove the new `vfs` scaffold, Phase 0 tests and ADR without changing existing runtime behavior |
| `86b96e7a7503decb19bdce59e3c20070058e0fac` | 1     | Lossless path/resource types, native codecs, v2 protobuf and `ProjectPath` dual-write | VFS/proto tests, cross-target checks, project adapter test, clippy and docs Prettier PASS; buf NOT RUN | Remove v2 fields/types and dual-write adapter while retaining legacy string behavior            |

## 平台 QA {#platform-qa}

| Platform           | Status  | Runbook/result                                    | Remaining risk                            |
| ------------------ | ------- | ------------------------------------------------- | ----------------------------------------- |
| Linux              | NOT RUN | Phase 1 codec/protobuf tests PASS                 | Full provider/consumer QA remains         |
| macOS              | NOT RUN | Phase 1 cross-target check PASS                   | Native filesystem behavior not executed   |
| Windows            | NOT RUN | Phase 1 cross-target check and WTF-16 corpus PASS | Native `OsString` generation not executed |
| WSL                | NOT RUN | -                                                 | -                                         |
| Docker remote      | NOT RUN | -                                                 | -                                         |
| SSH remote         | NOT RUN | -                                                 | -                                         |
| Network filesystem | NOT RUN | -                                                 | -                                         |

## Remaining work {#remaining-work}

- Complete Phase 2A commit, Phase 2B Local/Legacy providers and Phase 2C responsibility split.
- Phase 3 through Phase 10.

## Next action {#next-action}

提交 Phase 2A provider contract 和 MemoryProvider。随后实现 LegacyFsProvider 与
LocalProvider，让 shared conformance suite 覆盖 native positioned I/O、EOF、sparse
file、expected-version、paged listing、watch sequence/overflow 和 typed unsupported。
