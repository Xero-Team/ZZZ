# Vendored dependencies

The directories in this folder are source snapshots, not nested Git repositories or submodules.
[`UPSTREAMS.toml`](UPSTREAMS.toml) pins each original repository revision, selects the files kept in
the snapshot, and lists the ZZZ-specific patches applied after export.

Use the repository scripts from the workspace root:

```sh
# Report whether tracked branches or tags have moved to newer commits.
script/vendor-sync status

# Rebuild one or more snapshots from their pinned revisions.
script/vendor-sync sync reqwest scap

# Verify that every committed snapshot is reproducible.
script/check-vendor
```

When updating a package:

1. Resolve the intended upstream tag or maintenance branch to a commit SHA.
2. Update `source_ref` and `revision` in `UPSTREAMS.toml`.
3. Export the new source with `script/vendor-sync sync <package>`.
4. Reapply only the patches still required by ZZZ and refresh the package's patch files.
5. Update source comments, `Cargo.lock`, call sites, and platform-specific compatibility code.
6. Run `script/check-vendor`, `cargo fmt --all -- --check`, relevant tests, and `./script/clippy`.

Patch files use paths relative to the package root and are applied in manifest order. Keep upstream
license files in every snapshot. Do not add `.git` directories or depend on commits that exist only
in an unowned fork.
