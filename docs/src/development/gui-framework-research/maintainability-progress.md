# GPUI Maintainability — Progress Ledger {#progress}

本账本记录 [maintainability-plan.md](./maintainability-plan.md) 的逐阶段执行
证据。所有命令在本 Linux 主机（`rust-toolchain.toml` 固定 `1.99.1`）实跑；
原始日志在 `.tmp/gpui-maintainability/`（不 tracked）。

**提交状态**：工作树改动全部保留，**未 commit / 未 push / 未开 PR**，因此
各阶段 "commit SHA" 为空。基线 HEAD 为 `631a7a0d9d`。

## 阶段记录 {#phases}

| 阶段             | 状态       | 关键命令                                                | 结果                                               | 证据                           |
| ---------------- | ---------- | ------------------------------------------------------- | -------------------------------------------------- | ------------------------------ |
| 0 基线           | `COMPLETE` | `cargo check --locked -p gpui`                          | `PASS` 8.21 s                                      | `phase0-cargo-check.log`       |
| 1 API 门禁       | `COMPLETE` | `cargo xtask check gpui-api`                            | 零 diff，2609 条；负向测试捕获 `Colors::dark` 改名 | `crates/gpui/api-snapshot.txt` |
| 2 window 分区    | `COMPLETE` | `cargo test -p gpui --lib`、全 workspace / 跨平台 check | 291 passed；门禁零 diff；`window.rs` 8198→3052 行  | `phase2-*.log`                 |
| 3 non_exhaustive | `COMPLETE` | `cargo check --locked --workspace`、clippy、fmt         | `PASS`；4 类型 + 11 构造点                         | `phase3-workspace-after.log`   |
| 4 内核文档       | `COMPLETE` | `rg` 符号核验、prettier、`cargo xtask check`            | `PASS`；7 新页 + 1 扩写                            | 本文件 + `crates/gpui/docs/`   |
| 5 Result 化      | `COMPLETE` | 293 tests、全 workspace、三平台交叉、clippy、门禁 bless | `PASS`；13 处公开签名变更                          | `phase5-*.log`                 |
| 6 可审查性       | `COMPLETE` | `rg` / `git blame` / `--all-features` check             | 2 项完成，2 项 `DEFERRED`                          | 见计划阶段 6                   |
| 7 验证移交       | `COMPLETE` | 见下方矩阵                                              | 见下方矩阵；4 项 pre-existing 已确证               | 本文件                         |

## 验证矩阵（阶段 7） {#matrix}

| 命令                                                                                 | 结果                                           |
| ------------------------------------------------------------------------------------ | ---------------------------------------------- |
| `cargo xtask check`                                                                  | `PASS`（10 门禁，含 `gpui-api` 零 diff）       |
| `cargo xtask check gpui-api`                                                         | `PASS`（快照 2609 条，13 处签名 bless 已复核） |
| `cargo test --locked -p gpui --lib`                                                  | `PASS`（293 passed, 0 failed）                 |
| `cargo test --locked -p gpui --lib --features frame-diagnostics,accessibility`       | `FAIL`（5 个失败）— **基线既有**，见下方说明   |
| `cargo test --locked -p gpui_platform -p gpui_wgpu -p gpui_linux`                    | `PASS`（0 tests）                              |
| `cargo check --locked --workspace`                                                   | `PASS`                                         |
| `cargo check --locked -p gpui --all-targets --features test-support`                 | `PASS`                                         |
| `cargo check --locked -p gpui_macos --target x86_64-apple-darwin`                    | `PASS`                                         |
| `cargo check --locked -p gpui_windows --target x86_64-pc-windows-msvc`               | `PASS`                                         |
| `RUSTC_BOOTSTRAP=1 cargo check --locked -p gpui_web --target wasm32-unknown-unknown` | `PASS`                                         |
| `cargo xtask clippy`                                                                 | `BLOCKED`（pre-existing）— 见下方说明          |
| `cargo xtask clippy -p gpui -p gpui_macros -p zzz -p xtask`                          | `PASS`（阶段 5 改动范围，4 pass 无 warning）   |
| `cargo fmt --all -- --check`                                                         | `BLOCKED`（pre-existing）— 见下                |
| `rustfmt --check`（本计划改动的每个 .rs 文件）                                       | `PASS`（全部干净）                             |
| `git diff --check`                                                                   | `PASS`                                         |
| `cd docs && npx prettier --check src/`                                               | `PASS`                                         |
| `npx prettier --check ../crates/gpui/docs/*.md`                                      | `PASS`                                         |
| `cargo test --locked --workspace`                                                    | `NOT RUN`（可选，耗时长）                      |

`gpui_web` 的 wasm target 需 `RUSTC_BOOTSTRAP=1`：其传递依赖
`wasm_thread` 使用了 `#![feature(stdarch_wasm_atomic_wait)]`，pinned stable
编译器拒绝 `E0554`。这与本计划改动无关，基线即如此。

### 四项 pre-existing 结果的确证 {#preexisting}

分别用干净 worktree（`HEAD 631a7a0d9d`）跑测试对照、用 `git diff` 对比
用户已有改动区域后确证，均与本计划改动无关：

- **`--features frame-diagnostics,accessibility` 的 5 个失败是基线既有**：
  基线跑同样命令为 `306 passed; 5 failed`，失败名单与本工作树完全一致
  （`window::tests` 的 4 个 frame-diagnostics / epoch 测试 +
  `elements::deferred::tests::test_cached_reuse_keeps_entity_subscriptions`）。
  本工作树是 `308 passed; 5 failed`，多出的 2 个正是阶段 5 新增的
  `default_lifecycle_*` 测试（均通过）。涉及的 `deferred.rs` 与
  `window.rs` tests 模块均未被本计划改动。
- **全量 `cargo xtask clippy` 的 `unused import: ProjectPath` 是用户已有
  工作树改动**：`crates/acp_thread/src/acp_thread.rs:26` 相比 HEAD 新增了
  `ProjectPath` 导入，clippy 的 `--deny warnings` 把它升级为 error。基线
  HEAD 无该导入。本计划未改动 `acp_thread`，按纪律不触碰；改动范围的
  限定版 clippy 已通过。
- **全量 `cargo fmt --all -- --check` 的 4 个文件 diff 同样来自用户已有
  改动**：`acp_thread.rs`、`markdown_preview_view.rs`、
  `project/tests/integration/project_tests.rs`、
  `worktree/tests/integration/worktree_tests.rs`（后两个是用户新增的
  metadata / VFS root 测试）。对本计划实际编辑的每个 `.rs` 文件逐一运行
  `rustfmt --edition 2024 --check` 全部干净。按纪律不格式化用户文件。
- **收尾复核时 `cargo check --locked --workspace` 报
  `markdown_preview` 的 `E0597`**：该文件含用户进行中的功能改动（新增
  `project_path_for_active_editor` 等），是尚未完成编辑的借用生命周期
  错误；阶段 5 验证期间同一命令曾 `PASS`，说明是工作树动态变化。本计划
  改动的 crate 单独 `check` 全部通过，`gpui --all-targets` 通过。按纪律
  不修用户文件。

对照完成后 `git worktree remove /tmp/zzz-baseline --force` 已清理。

## 平台欠账 `NOT RUN` {#platform-not-run}

以下项在本 Linux 主机无法验证，沿用进度账本已有 runbook：

- macOS VoiceOver / native accessibility runtime（EXP-006）
- Windows Narrator / WGPU headless hardware runtime（EXP-003/006）
- macOS/Windows native IME runtime（EXP-009）
- Web IME candidate positioning（已显式 unsupported）
- `x86_64-pc-windows-gnu` target（未安装）

## Next actions {#next}

1. 人工 review 阶段 2 的 `window/` 分区与 11 处 `pub(super)`、阶段 5 的
   公开签名变更与约 96 处调用点收敛。
2. 若认可，按"每提交一个可独立回退的变化"拆分提交并 `git commit -s`。
3. 平台欠账项在对应原生 / 浏览器环境按 runbook 验证。

## Suggested .rules additions {#rules}

按 `.rules:174`，以下为可复用模式建议（**未直接修改 `.rules`**）：

- 平台 trait 中"后端可能不支持"的操作返回 `Result<()>` 并给默认
  `bail!("... unsupported")` 实现，优于 `()` + `log::warn` 的静默 no-op；
  新后端默认显式失败而非假成功。
- 公开 API 快照门禁只认 `syn::Visibility::Public` 且不含源文件路径，
  因此对文件搬迁 / `pub(super)` 调整免疫；公开签名变更必须显式 bless
  并逐条复核 diff。
