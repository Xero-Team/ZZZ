# Changelog

All notable changes to ZZZ are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and ZZZ follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
ZZZ has not cut a versioned release yet, so the changes below are unreleased.

## How entries are written

- Add a line under `Added`, `Changed`, `Removed`, or `Fixed`.
- Write it so a ZZZ user can understand it. Skip internal jargon.
- Credit the contributor with `by @handle` when it came from a pull request.
- When a change is absorbed from upstream Zed, cite the upstream pull
  request as `(zed#NNNNN)`. Most ZZZ commits are absorbed from Zed, so
  these references are the norm.
- `Removed` is not a footnote. It records the surfaces ZZZ deliberately
  does not carry, and it is part of the project's evidence that they stay
  gone.

## [Unreleased]

### Added

- Add `SECURITY.md`. Security reports are public and go in the issue
  tracker; there is no private channel.
- Add [The Mission](./docs/src/mission.md), describing why the fork exists.
- Add [Migrating from Zed](./docs/src/migrate/zed.md).
- Add [Keeping ZZZ Lean](./docs/src/development/bloat.md) and
  [GPUI: Ownership and Data Flow](./docs/src/development/ownership-and-data-flow.md).
- Add Forgejo pull request and issue templates.
- Add a changelog.

### Changed

- Require AI-assisted review for every contribution. Original features
  additionally require human review; upstream absorption needs only the AI
  review.
- Distribute ZZZ under AGPL-3.0-or-later. Upstream Zed license texts are
  retained for the parts they cover.

### Removed

- N/A
