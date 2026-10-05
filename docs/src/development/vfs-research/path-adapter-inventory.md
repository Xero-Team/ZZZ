---
title: VFS Path Adapter Inventory
description: Caller and removal inventory for temporary UTF-8 path compatibility adapters.
---

# VFS path adapter inventory

This inventory prevents temporary UTF-8 and numeric-ID adapters from becoming permanent VFS
identity. Counts are broad source matches at Phase 1 and are updated when each consumer group
migrates.

## Introduced adapters {#introduced-adapters}

| Adapter                                           | Current callers                      | Loss behavior                                                                                  | Removal phase |
| ------------------------------------------------- | ------------------------------------ | ---------------------------------------------------------------------------------------------- | ------------- |
| `provider_path_from_legacy_utf8`                  | `project::ProjectPath::to_proto`     | Infallible only for normalized UTF-8 input; invalid components return `CompatibilityPathError` | Phase 9       |
| `provider_path_to_legacy_utf8`                    | `project::ProjectPath::from_proto`   | POSIX non-UTF-8 and Windows surrogate components return `UnrepresentableComponent`             | Phase 9       |
| `WorktreeId` numeric value as temporary `MountId` | `project::ProjectPath` v2 dual-write | No byte loss, but identity lifetime remains tied to the old worktree ID                        | Phase 3       |
| v1/v2 `ProjectPath` dual-read                     | `BufferStore::handle_save_buffer`    | Rejects mismatched v1 and v2 paths instead of choosing one                                     | Phase 9       |

New code must not add another UTF-8 fallback. A caller that cannot migrate in its planned phase
must be added here with an owner and removal phase.

## Rust caller groups {#rust-caller-groups}

The Phase 1 detector is:

```sh
rg -n 'RelPath::from_proto|\.path\.to_proto\(\)|path\.as_ref\(\)\.to_proto\(\)|relative_path\.to_proto\(\)' \
  crates/worktree crates/project crates/workspace crates/editor crates/language crates/proto \
  --glob '*.rs'
```

| File                                    | Broad hits | Migration owner                   | Removal phase |
| --------------------------------------- | ---------: | --------------------------------- | ------------- |
| `crates/project/src/lsp_store.rs`       |         10 | `LspPathMapper`                   | Phase 6       |
| `crates/worktree/src/worktree.rs`       |          9 | snapshot/worktree provider bridge | Phase 3       |
| `crates/project/src/toolchain_store.rs` |          7 | `NativeExecutionContext`          | Phase 6       |
| `crates/project/src/worktree_store.rs`  |          4 | consumer/provider operations      | Phase 5       |
| `crates/project/src/project.rs`         |          4 | project resource identity         | Phase 5       |
| `crates/project/src/git_store.rs`       |          4 | Git native mapping                | Phase 6       |
| `crates/editor/src/items.rs`            |          2 | editor persistence                | Phase 5       |
| `crates/workspace/src/persistence.rs`   |          1 | mount descriptor persistence      | Phase 5       |

## Protobuf migration groups {#protobuf-migration-groups}

`worktree.ProjectPath.path` now has a v2 companion and is dual-written. The remaining string
fields stay wire-compatible until their owning phase provides a complete reader, writer and
downgrade policy.

| Group                      | Existing string surfaces                                                                    | Planned replacement                                        | Removal phase |
| -------------------------- | ------------------------------------------------------------------------------------------- | ---------------------------------------------------------- | ------------- |
| Worktree scan and metadata | `File.path`, `Entry.path`, worktree root/canonical paths, create/rename/copy settings paths | `ProviderPathV2`, `NativePathV2`, `ResourceIdV2`           | Phase 3/4     |
| Buffer and media transfer  | `buffer.proto`, `image.proto`, `download.proto` path fields and dedicated byte RPCs         | VFS resource operations and positioned reads               | Phase 5/9     |
| Git                        | repository native paths, repo paths and repeated path lists in `git.proto`                  | execution-host native mapping plus provider resource paths | Phase 6/9     |
| Language server            | document and buffer paths in `lsp.proto`                                                    | `LspPathMapper`                                            | Phase 6/9     |
| Toolchains/tasks           | native and relative worktree paths in `toolchain.proto`                                     | `NativeExecutionContext`                                   | Phase 6/9     |

The Phase 1 schema keeps unknown fields additive. Old peers continue reading field `2` of
`ProjectPath`; new peers require v1/v2 equality while both fields are present.

## Dependency decision {#dependency-decision}

`typed-path 0.12.3` is used only inside the ZZZ-owned native Windows parser. It is
`MIT OR Apache-2.0`, uses only its default `std` feature, and adds no async runtime or I/O. It
does not define VFS identity, serialization or public provider traits. The rejected alternative
was another hand-written Windows prefix parser; ZZZ still owns WTF-8 validation, component
invariants and the protobuf codec.

## Platform fixture status {#platform-fixture-status}

- Linux POSIX byte generation: PASS on the Phase 1 host.
- Windows WTF-16 fixtures: PASS through host-independent checked-in vectors.
- Native Windows `OsString` generation: NOT RUN on Linux. Run the Phase 10 Windows runbook and
  compare `NativePath::from_local_path` with the same corpus.
- macOS normalization and per-volume case behavior: NOT RUN in Phase 1; provider/platform tests
  remain required.
