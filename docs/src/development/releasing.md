---
title: Releasing ZZZ
description: "Where the ZZZ version lives, what follows it, and what the release gate enforces."
---

# Releasing ZZZ

ZZZ does not run a hosted release channel. There is no preview channel and
no publication step to a website; a release is a tagged commit and the
artifacts `.github/workflows/ci.yaml` builds for that tag.

## The version has one home

`crates/zzz/Cargo.toml` declares the version. Nothing else declares it, and
nothing derives it from a date or a commit count:

```toml
[package]
name = "zzz"
version = "1.29.0"
```

The version follows upstream Zed. When a sync absorbs a Zed release bump,
ZZZ takes the same number, so a reader can tell which upstream release a
ZZZ tree is based on.

## What follows the version

| Location                                | Relationship                                        |
| --------------------------------------- | --------------------------------------------------- |
| `crates/zzz/Cargo.toml` `[package]`     | The authority. Edit this first.                     |
| `Cargo.lock` `[[package]] name = "zzz"` | Must match. `cargo check -p zzz` refreshes it.      |
| `CHANGELOG.md`                          | `## [Unreleased]` collects entries until a release. |
| Git tag                                 | `v<version>`, for example `v1.29.0`.                |
| `compliance_check.yml`                  | Derives `v<version>-pre` for the compliance run.    |

`cargo xtask check release` enforces the first three: the manifest version
must be a plain `major.minor.patch` value, the lockfile entry must match it,
and `CHANGELOG.md` must carry `## [Unreleased]` with its `### Added`,
`### Changed`, `### Removed`, and `### Fixed` subsections.

## What a release does not require

- No version bump for an ordinary pull request. The number moves when a
  sync follows an upstream release, and that commit carries both the
  manifest change and the lockfile refresh.
- No edits to entries under a released heading. Only `## [Unreleased]`
  changes between tags.
- No separate release-notes document. The changelog entry and the pull
  request's `Release Notes:` line are the whole record; see
  [Release Notes](./release-notes.md) for how to word one.

## Cutting a tag

1. Confirm the gate is green: `cargo xtask check release`.
2. Confirm the tree builds from a clean checkout: `cargo check -p zzz`.
3. Tag the commit that carries the version: `git tag -s v<version>`.
4. Push the tag. CI builds the tarball, the remote server, the SBOMs, and
   the provenance attestation for it.

The tag is the only place the version becomes public. If the gate is red,
the tag describes a tree that was never checked, so fix the drift first.

## Where the release rules live

| Question                               | File                                      |
| -------------------------------------- | ----------------------------------------- |
| How do I word a changelog entry?       | `CHANGELOG.md`, "How entries are written" |
| How do I word a `Release Notes:` line? | [Release Notes](./release-notes.md)       |
| Why do we not publish a feed?          | [The Mission](../mission.md)              |
